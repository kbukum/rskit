"""Single-line, in-place progress rendering for crates.io publish waits.

A first release waits out the crates.io *new crate* budget (one token every ten
minutes), so a run can spend long stretches idle between uploads. Rather than
scrolling one log line per poll, this reporter draws a single bar that fills
toward the moment of the next publish — a "releasing soon" indicator — and
collapses to a one-line confirmation when the wait ends.

On a non-interactive stream (CI logs) the bar would just spam carriage returns,
so the reporter falls back to a small, bounded number of plain status lines.
"""

from __future__ import annotations

import sys
from typing import TextIO

from .formatting import format_bar, format_duration, format_percent

# Braille spinner frames advanced once per render tick so a long wait still
# visibly "moves" even while the countdown only changes once per second.
_SPINNER_FRAMES = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"

# ANSI: carriage return + clear-to-end-of-line, so each in-place redraw fully
# overwrites the previous (possibly longer) frame instead of leaving residue.
_CLEAR_LINE = "\r\x1b[2K"


class WaitReporter:
    """Render a rate-limit wait as an in-place "next publish in …" progress bar."""

    def __init__(
        self,
        *,
        stream: TextIO | None = None,
        isatty: bool | None = None,
        bar_width: int = 24,
        log_interval: float = 30.0,
    ) -> None:
        self._stream = stream if stream is not None else sys.stderr
        if isatty is None:
            probe = getattr(self._stream, "isatty", None)
            self._isatty = bool(probe()) if callable(probe) else False
        else:
            self._isatty = isatty
        self._bar_width = bar_width
        self._log_interval = log_interval
        self._frame = 0
        self._next_log_at = 0.0
        self._active = False

    def start(self, label: str, total: float, *, reason: str) -> None:
        """Begin a wait of ``total`` seconds described by ``reason``."""

        self._frame = 0
        self._next_log_at = 0.0
        self._active = total > 0
        if self._active and not self._isatty:
            self._write(f"==> {label}: {reason}; next publish in ~{format_duration(total)}\n")

    def update(self, label: str, elapsed: float, total: float) -> None:
        """Redraw progress for a wait that is ``elapsed`` of ``total`` seconds in."""

        if not self._active or total <= 0:
            return
        remaining = max(0.0, total - elapsed)
        # A backward wall-clock jump (NTP/manual set/suspend) can drive elapsed
        # out of [0, total]; clamp the value used for the percent and bar so the
        # display never shows a negative or >100% fill, while remaining above
        # stays derived from the true elapsed to keep the countdown honest.
        shown = min(max(elapsed, 0.0), total)
        if self._isatty:
            self._render_inplace(label, shown, total, remaining)
        elif elapsed >= self._next_log_at:
            self._next_log_at = elapsed + self._log_interval
            pct = format_percent(shown, total)
            self._write(f"    {label}: {pct} ready, {format_duration(remaining)} left\n")

    def finish(self, label: str) -> None:
        """Collapse the bar to a single confirmation line once the wait is over."""

        if not self._active:
            return
        if self._isatty:
            bar = format_bar(1.0, 1.0, width=self._bar_width)
            pct = format_percent(1.0, 1.0)
            self._write(f"{_CLEAR_LINE}\u2713 {label}  {bar} {pct}  publishing…\n")
        else:
            self._write(f"==> {label}: rate limit cleared; publishing\n")
        self._active = False

    def _render_inplace(self, label: str, elapsed: float, total: float, remaining: float) -> None:
        frame = _SPINNER_FRAMES[self._frame % len(_SPINNER_FRAMES)]
        self._frame += 1
        bar = format_bar(elapsed, total, width=self._bar_width)
        pct = format_percent(elapsed, total)
        line = f"{frame} {label}  {bar} {pct}  next publish in {format_duration(remaining)}"
        self._write(_CLEAR_LINE + line)

    def _write(self, text: str) -> None:
        self._stream.write(text)
        self._stream.flush()
