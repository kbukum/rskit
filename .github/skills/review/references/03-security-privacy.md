# Pass 03 — Security & privacy

A dedicated pass because a vibe-coded path that "just works" usually skips boundary validation, and rskit is shared infrastructure — a gap here propagates to every consumer. For a deeper sweep on security-sensitive changes, pair this with a dedicated security review; this pass is the standing baseline.

> **Run in a separate, clean-context agent** — never inline in the session that wrote the code. An independent reviewer re-derives every judgment from the code and the principles instead of trusting prior reasoning. A plan/spec may be passed in as a scope checklist only; it never excuses a baseline violation.

**Scope note.** *Changes mode:* trace each new input path from its trust boundary to its use. *Project mode:* audit the toolkit's external-facing surfaces (HTTP, process, storage/database adapters, auth, crypto) for the invariants below; see also [`docs/security-model.md`](../../../../docs/security-model.md).

## Checks

- **Validate at every trust boundary.** Untrusted input is validated before use; least-privilege and secure-by-default. An input that flows into a query, a path, a command, or a deserialization without validation is a blocker.
- **Injection-safe data access.** Parameterized queries only — never string-built SQL. Argv-only subprocess execution — no shell interpolation of untrusted input (route through `rskit-process`).
- **Token hygiene.** Tokens/credentials go in headers, never query strings; never logged. Redact sensitive fields in errors and logs.
- **Current crypto.** No deprecated/weak algorithms (MD5, SHA-1 for security, ECB, static IVs, hard-coded keys); use current primitives. Crypto belongs in `rskit-encryption` / `rskit-security`, not hand-rolled.
- **Data minimization.** Minimize, redact, and retention-bound sensitive data; do not persist or log more than needed.

## Detection starters

Read each hit to judge intent — these flag candidates, not verdicts.

```bash
# string-built SQL / shelled commands with interpolation
rg 'format!\(.*SELECT|format!\(.*INSERT|query\(.*\+|sh -c|"bash"|"sh"' core/ contrib/
# secrets in URLs/logs, or logging a token/password/secret
rg '(token|secret|password|api_?key)=' core/ contrib/
rg '(info|debug|warn|trace|error)!\(.*\b(token|secret|password|api_?key)\b' core/ contrib/
# weak crypto
rg '\b(md5|sha1|Md5|Sha1|ECB)\b' core/ contrib/
# hard-coded credentials
rg 'let .*(password|secret|api_?key|token)\s*=\s*"' core/ contrib/
```

Flag any unbounded read of untrusted input (set explicit size limits) and any path/selector from an untrusted source flowing into filesystem or process execution without validation.
