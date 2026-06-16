"""Tests for the streaming subprocess tee helper."""

from __future__ import annotations

import sys
import unittest

from . import support  # noqa: F401
from rskit_tool.process import run_streamed


class RunStreamedTests(unittest.TestCase):
    def test_tees_lines_to_sink_and_captures_combined_output(self) -> None:
        seen: list[str] = []
        completed = run_streamed(
            [sys.executable, "-c", "import sys; print('out'); print('err', file=sys.stderr)"],
            sink=seen.append,
        )

        self.assertEqual(completed.returncode, 0)
        # stderr is merged into stdout, so both lines are captured and teed live.
        self.assertIn("out", completed.stdout)
        self.assertIn("err", completed.stdout)
        self.assertEqual("".join(seen), completed.stdout)

    def test_nonzero_exit_is_returned_not_raised(self) -> None:
        completed = run_streamed(
            [sys.executable, "-c", "import sys; sys.stderr.write('boom\\n'); sys.exit(3)"],
            sink=lambda _line: None,
        )

        self.assertEqual(completed.returncode, 3)
        self.assertIn("boom", completed.stdout)


    def test_sink_failure_disables_teeing_but_drains_output(self) -> None:
        # A sink that fails mid-stream (e.g. BrokenPipe when a downstream reader
        # closes) must not abort draining the child pipe — that could leave the
        # pipe full and deadlock the child. Teeing stops; capture and the process
        # both complete.
        calls = {"count": 0}

        def flaky(_line: str) -> None:
            calls["count"] += 1
            raise BrokenPipeError("downstream closed")

        completed = run_streamed(
            [sys.executable, "-c", "print('a'); print('b'); print('c')"],
            sink=flaky,
        )

        self.assertEqual(completed.returncode, 0)
        self.assertIn("a", completed.stdout)
        self.assertIn("c", completed.stdout)
        # Teeing was disabled after the first failure, not retried per line.
        self.assertEqual(calls["count"], 1)


if __name__ == "__main__":
    unittest.main()
