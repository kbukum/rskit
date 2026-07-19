"""Semantic line-break wrapping shared by the prose checker.

Prose is wrapped at a 100-column soft ceiling. A paragraph that already fits on one line is left
untouched. A longer paragraph is filled greedily and broken at the last meaningful boundary that
fits — a sentence end first, otherwise a clause boundary (after a comma/semicolon/colon, around an
em or en dash, or before a coordinating conjunction). Breaks land only between whole tokens, never
inside a bracketed or parenthesized span (so an interval like [0, 1) stays whole), and an inline
span (Markdown link/image, autolink, or backtick code) is one atomic token that is never split. A
clause with no legal break point is emitted whole even if it exceeds the ceiling: the ceiling is
soft and never justifies a mid-clause or mid-span break.
"""

from __future__ import annotations

import re

WIDTH = 100

_ABBR = {
    "e.g", "i.e", "etc", "vs", "cf", "al", "resp", "approx", "fig", "eq",
    "no", "st", "mr", "mrs", "ms", "dr", "inc", "ltd", "co", "vol", "ch", "sec",
}

# An atom is an unbreakable inline span (linked image, image/link, autolink, backtick code) or a
# run of non-whitespace. Ordered so span forms win over the bare non-whitespace fallback.
_ATOM = re.compile(
    r"\[!\[[^\]]*\]\([^)]*\)\]\([^)]*\)\S*"   # [![alt](img)](link) linked image
    r"|!?\[[^\]]*\]\([^)]*\)\S*"              # [text](url) / ![alt](url)
    r"|`[^`]+`\S*"                            # `code`
    r"|<[^>\s]+>\S*"                          # <autolink>
    r"|\S+"                                   # plain word
)


def _atoms(text):
    return [m.group(0) for m in _ATOM.finditer(text)]


def _ends_sentence(atom):
    w = atom.rstrip("\"')]")
    stem = w.rstrip(".?!")
    if stem == w or not stem:
        return False
    last = stem.split("(")[-1]
    if last.lower() in _ABBR:
        return False
    if len(last) == 1 and last.isalpha() and last.isupper():
        return False
    return True


def _breakable_flags(atoms):
    n = len(atoms)
    flags = [False] * n
    depths = [0] * n
    depth = 0
    for i, w in enumerate(atoms):
        depths[i] = depth
        depth += w.count("(") + w.count("[") - w.count(")") - w.count("]")
        if depth < 0:
            depth = 0
    for i in range(1, n):
        if depths[i] != 0:
            continue
        prev, cur = atoms[i - 1], atoms[i]
        if _ends_sentence(prev):
            flags[i] = True
        elif prev[-1:] in (",", ";", ":") or prev in ("—", "–") or prev[-1:] in ("—", "–"):
            flags[i] = True
        elif cur in ("and", "or", "but", "so", "yet", "nor", "—", "–"):
            flags[i] = True
    return flags


def _width(atoms, a, b):
    if b <= a:
        return 0
    return sum(len(w) for w in atoms[a:b]) + (b - a - 1)


def wrap_semantic(text, first_prefix, cont_prefix, width=WIDTH):
    text = text.strip()
    if not text:
        return [first_prefix.rstrip()]
    if len(first_prefix) + len(text) <= width:
        return [(first_prefix + text).rstrip()]

    atoms = _atoms(text)
    n = len(atoms)
    breakable = _breakable_flags(atoms)
    budget = max(width - len(cont_prefix), 20)

    lines = []
    line_start = 0
    last_break = -1
    for i in range(1, n + 1):
        if _width(atoms, line_start, i) > budget and last_break > line_start:
            lines.append(" ".join(atoms[line_start:last_break]))
            line_start = last_break
            last_break = -1
            for j in range(line_start + 1, i):
                if j < n and breakable[j]:
                    last_break = j
        if i < n and breakable[i]:
            last_break = i
    lines.append(" ".join(atoms[line_start:n]))

    out = []
    for idx, ln in enumerate(lines):
        prefix = first_prefix if idx == 0 else cont_prefix
        out.append((prefix + ln).rstrip())
    return out
