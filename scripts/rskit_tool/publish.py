"""Idempotent, rate-aware crates.io publishing primitive.

The publisher mirrors the crates.io publish rate limits locally so a release run
*waits out* its budget instead of failing on a ``429``. It is also idempotent:
any crate whose ``name@version`` is already on crates.io is skipped, which makes
an interrupted release resumable with no manual bookkeeping.

Rate limits are keyed per publishing account (``(user_id, action)``), so every
crate shares one bucket per action class:

==================  =====  =================
Action              Burst  Refill
==================  =====  =================
new crate           5      1 per 10 minutes
update (existing)   30     1 per minute
==================  =====  =================
"""

from __future__ import annotations

import json
import re
import time
import urllib.error
import urllib.request
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field
from email.utils import parsedate_to_datetime

from .errors import ToolError

# Rate limits mirrored from the crates.io source (per publishing account).
NEW_CRATE_BURST = 5
NEW_CRATE_REFILL_SECONDS = 600.0  # 1 token per 10 minutes
UPDATE_BURST = 30
UPDATE_REFILL_SECONDS = 60.0  # 1 token per minute

# Cargo surfaces a crates.io rate-limit rejection in its error output; these
# markers let the publisher react to a 429 even though it rate-limits proactively.
# A bare "429" is intentionally excluded — it collides with unrelated numbers in
# cargo output (line/IDs) and the phrasings below already cover real rejections.
_RATE_LIMIT_MARKERS = (
    "too many requests",
    "rate limit",
    "published too many",
)
_RETRY_AFTER_RE = re.compile(r"try again (?:after|on)\s+(.+?)(?:\.|$)", re.IGNORECASE)

_USER_AGENT = "rskit-release (https://github.com/kbukum/rskit)"


@dataclass(frozen=True)
class CratePlan:
    """A single crate to publish, in dependency order."""

    name: str
    version: str
    manifest: str


@dataclass(frozen=True)
class CommandResult:
    """Outcome of a single ``cargo publish`` invocation."""

    returncode: int
    output: str


@dataclass
class PublishOutcome:
    """Summary of a publish run."""

    published: list[str] = field(default_factory=list)
    skipped: list[str] = field(default_factory=list)


class TokenBucket:
    """A refilling token bucket mirroring a crates.io publish budget."""

    def __init__(self, capacity: int, refill_seconds: float, *, now: Callable[[], float]) -> None:
        self._capacity = float(capacity)
        self._refill_seconds = refill_seconds
        self._tokens = float(capacity)
        self._now = now
        self._last = now()

    @property
    def refill_seconds(self) -> float:
        """Seconds to regain one token once the bucket is empty."""

        return self._refill_seconds

    def _refill(self) -> None:
        current = self._now()
        elapsed = current - self._last
        if elapsed > 0:
            self._tokens = min(self._capacity, self._tokens + elapsed / self._refill_seconds)
            self._last = current

    def time_until_available(self) -> float:
        """Return seconds to wait before a token is available (0 when ready)."""

        self._refill()
        if self._tokens >= 1.0:
            return 0.0
        return (1.0 - self._tokens) * self._refill_seconds

    def consume(self) -> None:
        """Spend one token, refilling first."""

        self._refill()
        self._tokens -= 1.0


def crate_version_published(crate: str, version: str) -> bool:
    """Return true when crates.io already has ``crate@version`` (404 -> false)."""

    url = f"https://crates.io/api/v1/crates/{crate}/{version}"
    data = _get_json(url, f"crates.io lookup for {crate} {version}")
    if data is None:
        return False
    return data.get("version", {}).get("num") == version


def crate_exists(crate: str) -> bool:
    """Return true when the crate *name* exists on crates.io at all (any version)."""

    url = f"https://crates.io/api/v1/crates/{crate}"
    data = _get_json(url, f"crates.io lookup for {crate}")
    return data is not None


def _get_json(url: str, context: str) -> dict | None:
    """GET a crates.io JSON resource, returning None on 404."""

    request = urllib.request.Request(url, headers={"User-Agent": _USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise ToolError(f"{context} failed: HTTP {error.code}") from error
    except Exception as error:  # noqa: BLE001 - network/JSON errors share one failure path.
        raise ToolError(f"{context} failed: {error}") from error


class CratesIoRegistry:
    """Live crates.io existence checks for the publisher."""

    def version_published(self, name: str, version: str) -> bool:
        """Return true when ``name@version`` already exists on crates.io."""

        return crate_version_published(name, version)

    def crate_exists(self, name: str) -> bool:
        """Return true when the crate name already exists on crates.io."""

        return crate_exists(name)


def is_rate_limited(output: str) -> bool:
    """Return true when ``cargo publish`` output indicates a crates.io rate limit."""

    lowered = output.lower()
    return any(marker in lowered for marker in _RATE_LIMIT_MARKERS)


def parse_retry_after(output: str, *, wall_now: float) -> float | None:
    """Parse a crates.io "try again after <date>" hint into seconds to wait."""

    match = _RETRY_AFTER_RE.search(output)
    if not match:
        return None
    try:
        target = parsedate_to_datetime(match.group(1).strip())
    except (TypeError, ValueError):
        return None
    if target is None:
        return None
    wait = target.timestamp() - wall_now
    return max(0.0, wait)


class RateAwarePublisher:
    """Publish crates idempotently, waiting out the crates.io rate budget."""

    def __init__(
        self,
        *,
        registry: CratesIoRegistry,
        publish_crate: Callable[[CratePlan], CommandResult],
        now: Callable[[], float] = time.monotonic,
        sleep: Callable[[float], None] = time.sleep,
        wall_now: Callable[[], float] = time.time,
        log: Callable[[str], None] = print,
        max_rate_retries: int = 8,
    ) -> None:
        self._registry = registry
        self._publish_crate = publish_crate
        self._sleep = sleep
        self._wall_now = wall_now
        self._log = log
        self._max_rate_retries = max_rate_retries
        self._new_bucket = TokenBucket(NEW_CRATE_BURST, NEW_CRATE_REFILL_SECONDS, now=now)
        self._update_bucket = TokenBucket(UPDATE_BURST, UPDATE_REFILL_SECONDS, now=now)

    def publish(self, crates: Sequence[CratePlan]) -> PublishOutcome:
        """Publish each crate not already on crates.io, in dependency order."""

        outcome = PublishOutcome()
        for plan in crates:
            if self._registry.version_published(plan.name, plan.version):
                self._log(f"==> skip {plan.name} {plan.version} (already on crates.io)")
                outcome.skipped.append(plan.name)
                continue
            is_new = not self._registry.crate_exists(plan.name)
            bucket = self._new_bucket if is_new else self._update_bucket
            self._publish_with_retry(plan, bucket, is_new)
            outcome.published.append(plan.name)
        return outcome

    def _acquire_token(self, plan: CratePlan, bucket: TokenBucket, is_new: bool) -> None:
        """Wait for and consume one token, so each publish attempt costs a token."""

        wait = bucket.time_until_available()
        if wait > 0:
            kind = "new-crate" if is_new else "update"
            self._log(
                f"==> {plan.name} {plan.version}: {kind} rate budget exhausted; "
                f"waiting {wait:.0f}s for a token"
            )
            self._sleep(wait)
        bucket.consume()

    def _publish_with_retry(self, plan: CratePlan, bucket: TokenBucket, is_new: bool) -> None:
        attempts = 0
        while True:
            # Every attempt — including a post-429 retry — spends a token so the
            # local model never runs ahead of the real crates.io budget.
            self._acquire_token(plan, bucket, is_new)
            self._log(f"==> cargo publish --locked {plan.name} {plan.version}")
            result = self._publish_crate(plan)
            if result.returncode == 0:
                return
            if is_rate_limited(result.output) and attempts < self._max_rate_retries:
                attempts += 1
                wait = parse_retry_after(result.output, wall_now=self._wall_now())
                if wait is None:
                    wait = bucket.refill_seconds
                self._log(
                    f"==> {plan.name} {plan.version}: crates.io rate limit hit; "
                    f"waiting {wait:.0f}s then retrying (attempt {attempts}/{self._max_rate_retries})"
                )
                self._sleep(wait)
                continue
            raise ToolError(
                f"cargo publish failed for {plan.name} {plan.version} "
                f"(exit {result.returncode}):\n{result.output.strip()}"
            )
