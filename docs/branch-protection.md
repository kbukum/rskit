# Branch Protection Settings

The `main` branch should be protected with:

- ✅ Require pull request reviews (1 approver minimum)
- ✅ Dismiss stale reviews on new commits
- ✅ Require status checks to pass:
  - `Rustfmt`
  - `Clippy`
  - `Test (ubuntu-latest / 1.97 / default)`
  - `Test (ubuntu-latest / 1.97 / all)`
  - `Test (macos-latest / 1.97 / all)`
  - `MSRV (ubuntu-latest / 1.97 / default)`
  - `MSRV (ubuntu-latest / 1.97 / all)`
  - `Release Guardrails`
  - `Security`
  - `CI Status`
- ✅ Require branches to be up to date before merging
- ✅ No force pushes
- ✅ No branch deletions

Configure at: https://github.com/kbukum/rskit/settings/branches
