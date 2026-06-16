"""Idempotent, rate-aware crates.io publishing primitive.

The publisher is *reactive-first*: it attempts each ``cargo publish`` straight
away and only waits when crates.io actually rejects the upload with a rate-limit
``429``. crates.io includes an authoritative ``try again after <date>`` hint on
those rejections, so the publisher schedules the wait to exactly that deadline.
When a rejection omits the hint it falls back to a bounded *probe* interval and
retries — a rejected publish does not consume budget, so probing never makes the
limit worse. It is also idempotent: any crate whose ``name@version`` is already
on crates.io is skipped, which makes an interrupted release resumable with no
manual bookkeeping.

Rate limits are keyed per publishing account (``(user_id, action)``):

==================  =====  =================
Action              Burst  Refill
==================  =====  =================
new crate           5      1 per 10 minutes
update (existing)   30     1 per minute
==================  =====  =================

The refill cadence above is the *fallback* probe interval (capped) used only
when a rejection carries no explicit retry-after; the server's hint is always
preferred when present.
"""

from __future__ import annotations

import json
import math
import re
import time
import urllib.error
import urllib.request
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field
from email.utils import parsedate_to_datetime

from .errors import ToolError
from .publish_progress import WaitReporter

# Fallback refill cadences mirrored from the crates.io source (per account).
# These only bound the probe interval when a 429 omits a retry-after hint.
NEW_CRATE_REFILL_SECONDS = 600.0  # 1 token per 10 minutes
UPDATE_REFILL_SECONDS = 60.0  # 1 token per minute

# Cargo surfaces a crates.io rate-limit rejection in its error output; these
# markers let the publisher detect a 429 so its reactive-first loop can wait.
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
    """Publish crates idempotently, scheduling waits around crates.io 429s."""

    def __init__(
        self,
        *,
        registry: CratesIoRegistry,
        publish_crate: Callable[[CratePlan], CommandResult],
        # crates.io retry-after hints are absolute wall-clock deadlines and the
        # budget keeps refilling while the host is suspended, so waits are derived
        # from the same wall basis as ``parse_retry_after`` rather than a monotonic
        # clock that can pause.
        sleep: Callable[[float], None] = time.sleep,
        wall_now: Callable[[], float] = time.time,
        log: Callable[[str], None] = print,
        progress: WaitReporter | None = None,
        max_rate_retries: int = 8,
        poll_interval: float = 1.0,
        probe_interval: float = 60.0,
    ) -> None:
        if not math.isfinite(poll_interval) or poll_interval <= 0:
            raise ValueError(
                f"poll_interval must be a positive, finite number of seconds, got {poll_interval!r}"
            )
        if not math.isfinite(probe_interval) or probe_interval <= 0:
            raise ValueError(
                f"probe_interval must be a positive, finite number of seconds, got {probe_interval!r}"
            )
        self._registry = registry
        self._publish_crate = publish_crate
        self._sleep = sleep
        self._wall_now = wall_now
        self._log = log
        self._progress = progress if progress is not None else WaitReporter()
        self._max_rate_retries = max_rate_retries
        self._poll_interval = poll_interval
        self._probe_interval = probe_interval

    def publish(self, crates: Sequence[CratePlan]) -> PublishOutcome:
        """Publish each crate not already on crates.io, in dependency order."""

        outcome = PublishOutcome()
        for plan in crates:
            if self._registry.version_published(plan.name, plan.version):
                self._log(f"==> skip {plan.name} {plan.version} (already on crates.io)")
                outcome.skipped.append(plan.name)
                continue
            is_new = not self._registry.crate_exists(plan.name)
            self._publish_with_retry(plan, is_new)
            outcome.published.append(plan.name)
        return outcome

    def _wait(self, seconds: float, *, reason: str, plan: CratePlan) -> None:
        """Sleep until ``seconds`` elapse, drawing an in-place countdown bar.

        Re-deriving the remaining time from a wall-clock deadline each slice keeps
        the wait honest if the wall clock jumps (e.g. the machine suspends
        mid-wait), instead of blindly over-sleeping one large interval.
        """

        if seconds <= 0:
            return
        label = f"{plan.name} {plan.version}"
        deadline = self._wall_now() + seconds
        self._progress.start(label, seconds, reason=reason)
        while True:
            remaining = deadline - self._wall_now()
            if remaining <= 0:
                break
            self._progress.update(label, seconds - remaining, seconds)
            self._sleep(min(self._poll_interval, remaining))
        self._progress.finish(label)

    def _fallback_wait(self, is_new: bool) -> float:
        """Bounded probe interval used when a 429 carries no retry-after hint.

        A rejected publish does not spend budget, so we re-probe at the smaller of
        the action's refill cadence and ``probe_interval`` instead of sleeping a
        full, opaque refill window we cannot confirm.
        """

        refill = NEW_CRATE_REFILL_SECONDS if is_new else UPDATE_REFILL_SECONDS
        return min(refill, self._probe_interval)

    def _publish_with_retry(self, plan: CratePlan, is_new: bool) -> None:
        attempts = 0
        while True:
            self._log(f"==> cargo publish --locked {plan.name} {plan.version}")
            result = self._publish_crate(plan)
            if result.returncode == 0:
                return
            if is_rate_limited(result.output) and attempts < self._max_rate_retries:
                attempts += 1
                scheduled = parse_retry_after(result.output, wall_now=self._wall_now())
                if scheduled is not None and scheduled > 0:
                    wait, reason = scheduled, "crates.io rate limit; scheduled from retry-after"
                else:
                    # No usable hint: the 429 either omitted a retry-after or it
                    # parsed to a now/past deadline (clock skew). Probing a bounded
                    # interval avoids a tight retry loop that would hammer crates.io
                    # and burn the retry budget near-instantly.
                    wait, reason = self._fallback_wait(is_new), "crates.io rate limit; probing budget"
                self._log(
                    f"==> {plan.name} {plan.version}: {reason} "
                    f"(retry {attempts}/{self._max_rate_retries})"
                )
                self._wait(wait, reason=reason, plan=plan)
                continue
            raise ToolError(
                f"cargo publish failed for {plan.name} {plan.version} "
                f"(exit {result.returncode}):\n{result.output.strip()}"
            )
