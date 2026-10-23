# Lens: security and runtime

rskit is shared infrastructure; a gap here reaches every consumer. For a deep audit, use a dedicated security review as well.

Rules: [security and runtime](../../../engineering.md#security-and-runtime), [AI features](../../../engineering.md#ai-features), [security model](../../../../docs/security-model.md).

## What to look for

- **Trust boundaries.** Trace each input from its boundary to its use. Validate before it reaches a query, path, command, or deserializer. Bound reads of untrusted input.
- **Injection.** Parameterized SQL only. Argv-only subprocesses through `rskit-process`.
- **Secrets.** Not in code, logs, errors, or URLs. Tokens in headers. Redact sensitive fields.
- **Crypto and paths.** Current algorithms from `rskit-encryption` / `rskit-security`. Root-confined paths use `rskit-fs` `confine_path`.
- **Errors.** No runtime `unwrap`, `expect`, `panic!`, swallowed errors, or success-shaped fallbacks. Causes preserved.
- **Remote calls.** Timeouts on every call. Retries only for idempotent work, bounded and jittered, through `rskit-resilience`.
- **Tasks and shutdown.** Every spawn has an owner, cancellation, and shutdown. Channels, queues, and concurrency are bounded. No lock or borrow held across `.await`; no blocking inside async.
- **Composition.** Injected registries and policies. No `static mut`, global registries, or import-time I/O.
- **`unsafe`.** Only where the policy allows it, each block with a `// SAFETY:` comment stating the real invariant.
- **AI.** Model output and retrieved context are untrusted; structured output validated; destructive tool calls gated by a human.

## Severity

| Severity | Examples |
|---|---|
| Blocker | Unvalidated input into SQL, path, or command; leaked secret; runtime panic path; unbounded channel or task without cancellation; global mutable registry; weak crypto |
| Should-fix | Missing timeout or unbounded retry; lock held across unrelated I/O; vague `SAFETY:` comment; missing redaction in an error message |
| Nit | Defensive check that a type already guarantees |

## Detection starters

Exclude `#[cfg(test)]` and `tests/` when judging runtime hits.

```bash
rg '\.unwrap\(\)|\.expect\(|panic!\(' core/*/src contrib/*/*/src
rg 'format!\(.*(SELECT|INSERT|UPDATE|DELETE)|"sh"|"bash"|sh -c' core/*/src contrib/*/*/src
rg -i '(token|secret|password|api_?key)=' core/*/src contrib/*/*/src
rg -i '(info|debug|warn|trace|error)!\(.*(token|secret|password|api_?key)' core/*/src contrib/*/*/src
rg -w 'md5|sha1|Md5|Sha1|Ecb' core/*/src contrib/*/*/src
rg 'unbounded_channel|tokio::spawn|static mut|lazy_static' core/*/src contrib/*/*/src
rg -n -B2 'unsafe ' core/*/src contrib/*/*/src             # read each SAFETY comment
toven run readiness                                          # panic, SAFETY, pinning, audit gates
```
