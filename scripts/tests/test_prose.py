"""Tests for the semantic-line-break prose checker (scripts/check-prose.py)."""

from __future__ import annotations

import importlib.util
import unittest

from . import support

_SPEC = importlib.util.spec_from_file_location(
    "check_prose", support.SCRIPTS / "check-prose.py"
)
assert _SPEC and _SPEC.loader
check_prose = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(check_prose)

reflow_markdown = check_prose.reflow_markdown
reflow_rust = check_prose.reflow_rust
LONG = (
    "This is a fairly long sentence that comfortably exceeds one hundred columns so the checker "
    "must break it, and it also has a trailing clause after a comma. A second sentence follows."
)


class MarkdownReflowTests(unittest.TestCase):
    def test_long_paragraph_breaks_at_boundaries(self) -> None:
        reflowed = reflow_markdown(LONG)
        lines = reflowed.split("\n")
        self.assertGreater(len(lines), 1)
        for line in lines:
            self.assertLessEqual(len(line), 100)
        # A break must land at a sentence boundary: the second sentence starts a new line.
        self.assertTrue(any(line.startswith("A second sentence") for line in lines))

    def test_never_breaks_mid_clause(self) -> None:
        # A single word longer than the ceiling has no legal break and is emitted whole.
        word = "x" * 120
        self.assertEqual(reflow_markdown(word), word)

    def test_table_and_list_preserved(self) -> None:
        src = (
            "| a | b |\n"
            "| - | - |\n"
            "| 1 | 2 |\n"
            "\n"
            "- item one\n"
            "- item two\n"
        )
        self.assertEqual(reflow_markdown(src), src)

    def test_frontmatter_preserved(self) -> None:
        src = (
            "---\n"
            "name: demo\n"
            "description: >-\n"
            "    an intentionally long folded scalar that should be left exactly as written verbatim\n"
            "---\n"
            "\n"
            "Body paragraph.\n"
        )
        self.assertEqual(reflow_markdown(src), src)

    def test_fenced_code_preserved(self) -> None:
        src = (
            "```rust\n"
            "let x = some_really_long_identifier + another_long_identifier + yet_another_one_here;\n"
            "```\n"
        )
        self.assertEqual(reflow_markdown(src), src)

    def test_idempotent(self) -> None:
        once = reflow_markdown(LONG)
        twice = reflow_markdown(once)
        self.assertEqual(once, twice)


class RustReflowTests(unittest.TestCase):
    def test_outer_doc_reflows(self) -> None:
        src = f"/// {LONG}\n"
        reflowed = reflow_rust(src)
        lines = reflowed.split("\n")
        self.assertGreater(len([ln for ln in lines if ln.strip()]), 1)
        for line in lines:
            self.assertTrue(line == "" or line.startswith("///"))
            self.assertLessEqual(len(line), 100)

    def test_inner_and_line_comment_markers_preserved(self) -> None:
        inner = reflow_rust(f"//! {LONG}\n")
        self.assertTrue(all(ln == "" or ln.startswith("//!") for ln in inner.split("\n")))
        line = reflow_rust(f"// {LONG}\n")
        self.assertTrue(all(ln == "" or ln.startswith("// ") or ln == "//" for ln in line.split("\n")))

    def test_marker_types_do_not_merge(self) -> None:
        # An outer-doc line and a following plain comment stay distinct blocks.
        src = "/// doc line\n// plain line\n"
        self.assertEqual(reflow_rust(src), src)

    def test_rustdoc_code_fence_preserved(self) -> None:
        src = (
            "/// Example:\n"
            "///\n"
            "/// ```\n"
            "/// let x = some_really_long_identifier + another_long_identifier + a_third_identifier;\n"
            "/// ```\n"
        )
        self.assertEqual(reflow_rust(src), src)

    def test_indented_rustdoc_code_preserved(self) -> None:
        src = (
            "///     let x = some_really_long_identifier + another_long_identifier + a_third_id_here;\n"
        )
        self.assertEqual(reflow_rust(src), src)

    def test_divider_line_preserved(self) -> None:
        src = "// ----------------------------------------------------------------\n"
        self.assertEqual(reflow_rust(src), src)

    def test_non_comment_code_untouched(self) -> None:
        src = "fn main() {\n    let value = compute();\n}\n"
        self.assertEqual(reflow_rust(src), src)

    def test_rustdoc_table_preserved(self) -> None:
        # rustdoc renders Markdown, so a table inside a doc comment must not be collapsed.
        src = (
            "//! | Module | Extra crate |\n"
            "//! |--------|-------------|\n"
            "//! | `util` | `rskit-util` |\n"
            "//! | `errors` | `rskit-errors` |\n"
        )
        self.assertEqual(reflow_rust(src), src)

    def test_rustdoc_list_preserved(self) -> None:
        src = (
            "/// Steps:\n"
            "///\n"
            "/// - first item\n"
            "/// - second item\n"
        )
        self.assertEqual(reflow_rust(src), src)

    def test_idempotent(self) -> None:
        once = reflow_rust(f"/// {LONG}\n")
        twice = reflow_rust(once)
        self.assertEqual(once, twice)


if __name__ == "__main__":
    unittest.main()
