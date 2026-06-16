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


if __name__ == "__main__":
    unittest.main()
