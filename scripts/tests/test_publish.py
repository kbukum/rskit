"""Tests for the idempotent, rate-aware crates.io publisher."""

from __future__ import annotations

import unittest

from . import support  # noqa: F401
from rskit_tool.errors import ToolError
from rskit_tool.publish import (
    NEW_CRATE_BURST,
    NEW_CRATE_REFILL_SECONDS,
    UPDATE_REFILL_SECONDS,
    CommandResult,
    CratePlan,
    RateAwarePublisher,
    TokenBucket,
    is_rate_limited,
    parse_retry_after,
)


class FakeClock:
    """A manually advanced monotonic clock that records sleeps."""

    def __init__(self) -> None:
        self.value = 0.0
        self.sleeps: list[float] = []

    def now(self) -> float:
        return self.value

    def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.value += seconds


class FakeRegistry:
    """In-memory crates.io stand-in for the publisher."""

    def __init__(self, *, published: set[tuple[str, str]] | None = None, names: set[str] | None = None) -> None:
        self.published = published or set()
        self.names = names or set()

    def version_published(self, name: str, version: str) -> bool:
        return (name, version) in self.published

    def crate_exists(self, name: str) -> bool:
        return name in self.names


def _plan(name: str, version: str = "0.1.0") -> CratePlan:
    return CratePlan(name=name, version=version, manifest=f"{name}/Cargo.toml")


class TokenBucketTests(unittest.TestCase):
    def test_burst_is_available_immediately(self) -> None:
        clock = FakeClock()
        bucket = TokenBucket(3, 60.0, now=clock.now)
        for _ in range(3):
            self.assertEqual(bucket.time_until_available(), 0.0)
            bucket.consume()

    def test_empty_bucket_reports_full_refill_interval(self) -> None:
        clock = FakeClock()
        bucket = TokenBucket(1, 600.0, now=clock.now)
        bucket.consume()
        self.assertEqual(bucket.time_until_available(), 600.0)

    def test_partial_elapsed_reduces_wait(self) -> None:
        clock = FakeClock()
        bucket = TokenBucket(1, 600.0, now=clock.now)
        bucket.consume()
        clock.value += 200.0
        self.assertAlmostEqual(bucket.time_until_available(), 400.0)


class ParseTests(unittest.TestCase):
    def test_is_rate_limited_detects_429(self) -> None:
        self.assertTrue(is_rate_limited("the remote server responded with status 429 Too Many Requests"))
        self.assertFalse(is_rate_limited("error: failed to verify package tarball"))

    def test_parse_retry_after_returns_seconds(self) -> None:
        # Tue, 01 Jan 2030 00:00:00 GMT is the target; wall clock 60s before it.
        target = "Tue, 01 Jan 2030 00:00:00 GMT"
        from email.utils import parsedate_to_datetime

        epoch = parsedate_to_datetime(target).timestamp()
        wait = parse_retry_after(f"please try again after {target}.", wall_now=epoch - 60)
        self.assertAlmostEqual(wait, 60.0)

    def test_parse_retry_after_missing_hint_is_none(self) -> None:
        self.assertIsNone(parse_retry_after("status 429 with no date", wall_now=0.0))


class RateAwarePublisherTests(unittest.TestCase):
    def _publisher(self, registry: FakeRegistry, publish_crate, clock: FakeClock) -> RateAwarePublisher:
        return RateAwarePublisher(
            registry=registry,
            publish_crate=publish_crate,
            now=clock.now,
            sleep=clock.sleep,
            wall_now=clock.now,
            log=lambda _message: None,
        )

    def test_skips_already_published_versions(self) -> None:
        clock = FakeClock()
        registry = FakeRegistry(published={("rskit-errors", "0.1.0")}, names={"rskit-errors"})
        attempted: list[str] = []

        def publish_crate(plan: CratePlan) -> CommandResult:
            attempted.append(plan.name)
            return CommandResult(0, "ok")

        outcome = self._publisher(registry, publish_crate, clock).publish([_plan("rskit-errors")])

        self.assertEqual(attempted, [])
        self.assertEqual(outcome.skipped, ["rskit-errors"])
        self.assertEqual(outcome.published, [])

    def test_publishes_unpublished_crate(self) -> None:
        clock = FakeClock()
        registry = FakeRegistry()
        published: list[str] = []

        def publish_crate(plan: CratePlan) -> CommandResult:
            published.append(plan.name)
            return CommandResult(0, "ok")

        outcome = self._publisher(registry, publish_crate, clock).publish([_plan("rskit-errors")])

        self.assertEqual(published, ["rskit-errors"])
        self.assertEqual(outcome.published, ["rskit-errors"])

    def test_new_crate_burst_then_waits_for_refill(self) -> None:
        clock = FakeClock()
        registry = FakeRegistry()  # nothing exists yet -> every crate is "new"

        def publish_crate(plan: CratePlan) -> CommandResult:
            return CommandResult(0, "ok")

        crates = [_plan(f"rskit-crate-{index}") for index in range(NEW_CRATE_BURST + 1)]
        self._publisher(registry, publish_crate, clock).publish(crates)

        # First NEW_CRATE_BURST publishes are free; the next waits one refill window.
        self.assertEqual(clock.sleeps, [NEW_CRATE_REFILL_SECONDS])

    def test_existing_crate_uses_update_budget(self) -> None:
        clock = FakeClock()
        registry = FakeRegistry(names={"rskit-errors"})  # name exists -> update budget
        buckets_waited: list[float] = []

        def publish_crate(plan: CratePlan) -> CommandResult:
            return CommandResult(0, "ok")

        # Update burst is 30, so two updates publish without waiting.
        self._publisher(registry, publish_crate, clock).publish(
            [_plan("rskit-errors", "0.2.0"), _plan("rskit-errors", "0.3.0")]
        )
        self.assertEqual(clock.sleeps, [])
        self.assertEqual(buckets_waited, [])

    def test_retries_after_rate_limit_then_succeeds(self) -> None:
        clock = FakeClock()
        registry = FakeRegistry(names={"rskit-errors"})
        calls = {"count": 0}

        def publish_crate(plan: CratePlan) -> CommandResult:
            calls["count"] += 1
            if calls["count"] == 1:
                return CommandResult(1, "status 429 Too Many Requests")
            return CommandResult(0, "ok")

        outcome = self._publisher(registry, publish_crate, clock).publish([_plan("rskit-errors", "0.2.0")])

        self.assertEqual(calls["count"], 2)
        self.assertEqual(outcome.published, ["rskit-errors"])
        # Fell back to the update refill interval since no Retry-After date was given.
        self.assertEqual(clock.sleeps, [UPDATE_REFILL_SECONDS])

    def test_real_publish_error_raises(self) -> None:
        clock = FakeClock()
        registry = FakeRegistry()

        def publish_crate(plan: CratePlan) -> CommandResult:
            return CommandResult(101, "error: failed to verify package tarball")

        with self.assertRaises(ToolError):
            self._publisher(registry, publish_crate, clock).publish([_plan("rskit-errors")])


if __name__ == "__main__":
    unittest.main()
