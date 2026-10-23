# Mode: review changes

Review a branch, commit range, or working-tree change.

1. **Scope.** Run `git diff <base>...HEAD --stat` (add `git diff` and `git diff --cached` when uncommitted work is in scope). Read each changed file, its callers and callees, and the contracts it touches. A core public change reaches the facade, adapters, and gokit parity; list that impact area.
2. **Lenses.** Apply each lens whose trigger matches, in the order in [the review skill](../SKILL.md#lenses). Defects already in the impact area are in scope; unrelated code is not.
3. **Gates.** Run the scoped gates the [validate skill](../../validate/SKILL.md) maps to the change, such as `make lint C=<crate>`, `make test C=<crate> T=<pattern>`, `make test-affected`, `make doc C=<crate>`, and `make check-public-api` for public changes. Leave the full `make check` for sign-off.
4. **Report** findings in the [one-line format](../SKILL.md#findings), plus any checks you skipped.
