"""Tests for the idempotent, rate-aware crates.io publisher."""

from __future__ import annotations

import unittest
from email.utils import formatdate

from . import support  # noqa: F401
from rskit_tool.errors import ToolError
from rskit_tool.publish import (
    NEW_CRATE_REFILL_SECONDS,
    UPDATE_REFILL_SECONDS,
    CommandResult,
    CratePlan,
    RateAwarePublisher,
    is_rate_limited,
    parse_retry_after,
)


class FakeClock:
    """A manually advanced wall clock that records sleeps."""

    def __init__(self, start: float = 0.0) -> None:
        self.value = start
        self.sleeps: list[float] = []

    def now(self) -> float:
        return self.value

    def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.value += seconds


class NullReporter:
    """A no-op wait reporter so tests stay silent and side-effect free."""

    def start(self, label: str, total: float, *, reason: str) -> None:
        pass

    def update(self, label: str, elapsed: float, total: float) -> None:
        pass

    def finish(self, label: str) -> None:
        pass


class FakeRegistry:
    """In-memory crates.io stand-in for the publisher."""

    def __init__(
        self, *, published: set[tuple[str, str]] | None = None, names: set[str] | None = None
    ) -> None:
        self.published = set() if published is None else published
        self.names = set() if names is None else names

    def version_published(self, name: str, version: str) -> bool:
        return (name, version) in self.published

    def crate_exists(self, name: str) -> bool:
        return name in self.names


def _plan(name: str, version: str = "0.1.0") -> CratePlan:
    return CratePlan(name=name, version=version, manifest=f"{name}/Cargo.toml")


def _rate_limited(retry_after: str | None = None) -> CommandResult:
    message = "You have published too many new crates"
    if retry_after is not None:
        message += f"; please try again after {retry_after}."
    return CommandResult(1, message)


class ParseTests(unittest.TestCase):
    def test_is_rate_limited_detects_429(self) -> None:
        self.assertTrue(
            is_rate_limited("the remote server responded with status 429 Too Many Requests")
        )
        self.assertFalse(is_rate_limited("error: failed to verify package tarball"))

    def test_is_rate_limited_ignores_bare_429_number(self) -> None:
        # A bare "429" in unrelated output (e.g. a line/ID) must not be treated
        # as a rate-limit rejection; only the real phrasings should match.
        self.assertFalse(is_rate_limited("error[E0429]: compile failure at line 429"))
        self.assertTrue(is_rate_limited("You have published too many new crates"))

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
    def _publisher(
        self,
        registry: FakeRegistry,
        publish_crate,
        clock: FakeClock,
        *,
        poll_interval: float = 10.0,
        probe_interval: float = 60.0,
        max_rate_retries: int = 8,
    ) -> RateAwarePublisher:
        return RateAwarePublisher(
            registry=registry,
            publish_crate=publish_crate,
            sleep=clock.sleep,
            wall_now=clock.now,
            log=lambda _message: None,
            progress=NullReporter(),
            poll_interval=poll_interval,
            probe_interval=probe_interval,
            max_rate_retries=max_rate_retries,
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

    def test_successful_new_crate_publishes_without_waiting(self) -> None:
        # Reactive-first: a publish that succeeds never pre-waits on a local budget.
        clock = FakeClock()
        registry = FakeRegistry()  # nothing exists yet -> "new" crate
        published: list[str] = []

        def publish_crate(plan: CratePlan) -> CommandResult:
            published.append(plan.name)
            return CommandResult(0, "ok")

        outcome = self._publisher(registry, publish_crate, clock).publish([_plan("rskit-errors")])

        self.assertEqual(published, ["rskit-errors"])
        self.assertEqual(outcome.published, ["rskit-errors"])
        self.assertEqual(clock.sleeps, [])

    def test_burst_of_new_crates_does_not_wait_until_rejected(self) -> None:
        # The first crates publish freely; the publisher only waits once crates.io
        # actually rejects an upload, not on a mirrored local burst count.
        clock = FakeClock()
        registry = FakeRegistry()

        def publish_crate(plan: CratePlan) -> CommandResult:
            return CommandResult(0, "ok")

        crates = [_plan(f"rskit-crate-{index}") for index in range(6)]
        self._publisher(registry, publish_crate, clock).publish(crates)

        self.assertEqual(clock.sleeps, [])

    def test_schedules_wait_from_server_retry_after(self) -> None:
        # When crates.io supplies an explicit retry-after, the wait is scheduled to
        # exactly that deadline rather than a guessed interval.
        base = 1_900_000_000.0
        clock = FakeClock(base)
        registry = FakeRegistry()
        retry_after = formatdate(base + 120, usegmt=True)
        calls = {"count": 0}

        def publish_crate(plan: CratePlan) -> CommandResult:
            calls["count"] += 1
            if calls["count"] == 1:
                return _rate_limited(retry_after)
            return CommandResult(0, "ok")

        self._publisher(registry, publish_crate, clock).publish([_plan("rskit-a")])

        self.assertEqual(calls["count"], 2)
        self.assertAlmostEqual(sum(clock.sleeps), 120.0)

    def test_probes_bounded_interval_when_no_retry_after(self) -> None:
        # Without an explicit retry-after we re-probe at the bounded probe interval
        # (min of the action refill and probe_interval), not a blind full window.
        clock = FakeClock()
        registry = FakeRegistry()  # new crate -> refill 600s, capped by probe 30s
        calls = {"count": 0}

        def publish_crate(plan: CratePlan) -> CommandResult:
            calls["count"] += 1
            if calls["count"] == 1:
                return _rate_limited()
            return CommandResult(0, "ok")

        self._publisher(registry, publish_crate, clock, probe_interval=30.0).publish(
            [_plan("rskit-a")]
        )

        self.assertEqual(calls["count"], 2)
        self.assertAlmostEqual(sum(clock.sleeps), 30.0)
        self.assertLess(30.0, NEW_CRATE_REFILL_SECONDS)

    def test_update_probe_uses_update_refill(self) -> None:
        clock = FakeClock()
        registry = FakeRegistry(names={"rskit-errors"})  # existing crate -> update refill 60s
        calls = {"count": 0}

        def publish_crate(plan: CratePlan) -> CommandResult:
            calls["count"] += 1
            if calls["count"] == 1:
                return _rate_limited()
            return CommandResult(0, "ok")

        self._publisher(registry, publish_crate, clock, probe_interval=600.0).publish(
            [_plan("rskit-errors", "0.2.0")]
        )

        self.assertAlmostEqual(sum(clock.sleeps), UPDATE_REFILL_SECONDS)

    def test_past_retry_after_falls_back_to_bounded_probe(self) -> None:
        # A retry-after that parses to now/the past (clock skew) must not collapse
        # to a zero wait and hammer crates.io; it falls back to the bounded probe
        # interval instead of tight-looping through the retry budget.
        base = 1_900_000_000.0
        clock = FakeClock(base)
        registry = FakeRegistry()
        retry_after = formatdate(base - 30, usegmt=True)  # already elapsed
        calls = {"count": 0}

        def publish_crate(plan: CratePlan) -> CommandResult:
            calls["count"] += 1
            if calls["count"] == 1:
                return _rate_limited(retry_after)
            return CommandResult(0, "ok")

        self._publisher(registry, publish_crate, clock, probe_interval=30.0).publish(
            [_plan("rskit-a")]
        )

        self.assertEqual(calls["count"], 2)
        self.assertAlmostEqual(sum(clock.sleeps), 30.0)

    def test_long_wait_is_sliced_into_poll_interval_chunks(self) -> None:
        # A long scheduled wait must be sliced so the bar updates and a clock jump
        # can be re-evaluated, never one opaque multi-minute sleep.
        base = 1_900_000_000.0
        clock = FakeClock(base)
        registry = FakeRegistry()
        retry_after = formatdate(base + 600, usegmt=True)
        calls = {"count": 0}

        def publish_crate(plan: CratePlan) -> CommandResult:
            calls["count"] += 1
            if calls["count"] == 1:
                return _rate_limited(retry_after)
            return CommandResult(0, "ok")

        self._publisher(registry, publish_crate, clock, poll_interval=10.0).publish(
            [_plan("rskit-a")]
        )

        self.assertTrue(all(slice_ <= 10.0 for slice_ in clock.sleeps), clock.sleeps)
        self.assertGreater(len(clock.sleeps), 1)
        self.assertAlmostEqual(sum(clock.sleeps), 600.0)

    def test_gives_up_after_max_rate_retries(self) -> None:
        base = 1_900_000_000.0
        clock = FakeClock(base)
        registry = FakeRegistry()
        retry_after = formatdate(base + 10, usegmt=True)
        calls = {"count": 0}

        def publish_crate(plan: CratePlan) -> CommandResult:
            calls["count"] += 1
            return _rate_limited(retry_after)

        with self.assertRaises(ToolError):
            self._publisher(registry, publish_crate, clock, max_rate_retries=2).publish(
                [_plan("rskit-a")]
            )

        # Two waits, then one final attempt that is no longer retried.
        self.assertEqual(calls["count"], 3)

    def test_real_publish_error_raises(self) -> None:
        clock = FakeClock()
        registry = FakeRegistry()

        def publish_crate(plan: CratePlan) -> CommandResult:
            return CommandResult(101, "error: failed to verify package tarball")

        with self.assertRaises(ToolError):
            self._publisher(registry, publish_crate, clock).publish([_plan("rskit-errors")])

    def test_non_positive_intervals_are_rejected(self) -> None:
        clock = FakeClock()
        registry = FakeRegistry()

        def publish_crate(plan: CratePlan) -> CommandResult:
            return CommandResult(0, "ok")

        for invalid in (0.0, -1.0, float("nan"), float("inf")):
            with self.assertRaises(ValueError):
                self._publisher(registry, publish_crate, clock, poll_interval=invalid)
            with self.assertRaises(ValueError):
                self._publisher(registry, publish_crate, clock, probe_interval=invalid)


if __name__ == "__main__":
    unittest.main()
