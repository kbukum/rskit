"""Tests for the in-place crates.io publish wait progress reporter."""

from __future__ import annotations

import io
import unittest

from . import support  # noqa: F401
from rskit_tool.publish_progress import WaitReporter


class WaitReporterTests(unittest.TestCase):
    def _reporter(self, *, isatty: bool) -> tuple[WaitReporter, io.StringIO]:
        stream = io.StringIO()
        reporter = WaitReporter(stream=stream, isatty=isatty, bar_width=10, log_interval=30.0)
        return reporter, stream

    def test_tty_renders_inplace_bar_with_countdown(self) -> None:
        reporter, stream = self._reporter(isatty=True)
        reporter.start("rskit-a 0.1.0", 120.0, reason="rate limit")
        reporter.update("rskit-a 0.1.0", 30.0, 120.0)
        reporter.update("rskit-a 0.1.0", 90.0, 120.0)
        reporter.finish("rskit-a 0.1.0")
        output = stream.getvalue()

        # In-place frames use a carriage return and the bar fills toward 100%.
        self.assertIn("\r", output)
        self.assertIn("next publish in", output)
        self.assertIn("25.0%", output)  # 30 / 120
        self.assertIn("75.0%", output)  # 90 / 120
        self.assertIn("100.0%", output)  # finish line, via shared formatter
        self.assertIn("publishing", output)

    def test_non_tty_emits_bounded_plain_lines(self) -> None:
        reporter, stream = self._reporter(isatty=False)
        reporter.start("rskit-a 0.1.0", 90.0, reason="rate limit")
        # Updates are throttled to log_interval, so many ticks collapse to few lines.
        for elapsed in range(0, 90, 5):
            reporter.update("rskit-a 0.1.0", float(elapsed), 90.0)
        reporter.finish("rskit-a 0.1.0")
        output = stream.getvalue()
        lines = [line for line in output.splitlines() if line.strip()]

        # No carriage-return animation when not a TTY (clean CI logs).
        self.assertNotIn("\r", output)
        self.assertIn("next publish in", output)  # start line
        self.assertIn("publishing", output)  # finish line
        # Throttled: a 90s wait at 30s cadence yields only a handful of lines.
        self.assertLessEqual(len(lines), 6)

    def test_backward_clock_jump_clamps_displayed_progress(self) -> None:
        # A backward wall-clock jump can hand update() a negative elapsed; the
        # percent/bar must clamp to 0% rather than render a negative fill, while
        # the remaining countdown stays honest (derived from the true elapsed).
        reporter, stream = self._reporter(isatty=True)
        reporter.start("rskit-a 0.1.0", 120.0, reason="rate limit")
        reporter.update("rskit-a 0.1.0", -30.0, 120.0)
        output = stream.getvalue()

        self.assertIn("0.0%", output)
        self.assertNotIn("-25.0%", output)  # the unclamped percent
        # remaining stays honest: 120 - (-30) = 150s -> 2m30s left.
        self.assertIn("next publish in 2m30s", output)

    def test_subsecond_wait_does_not_render_as_complete(self) -> None:
        # A sub-second wait (e.g. a retry-after a few hundred ms out) must not be
        # truncated so total rounds to 0 and shows a full/100% bar while waiting.
        reporter, stream = self._reporter(isatty=True)
        reporter.start("rskit-a 0.1.0", 0.8, reason="rate limit")
        reporter.update("rskit-a 0.1.0", 0.2, 0.8)
        output = stream.getvalue()

        self.assertIn("25.0%", output)  # 0.2 / 0.8, not 100%
        self.assertNotIn("100.0%", output)

    def test_zero_total_is_inert(self) -> None:
        reporter, stream = self._reporter(isatty=True)
        reporter.start("rskit-a 0.1.0", 0.0, reason="rate limit")
        reporter.update("rskit-a 0.1.0", 0.0, 0.0)
        reporter.finish("rskit-a 0.1.0")
        self.assertEqual(stream.getvalue(), "")


if __name__ == "__main__":
    unittest.main()
