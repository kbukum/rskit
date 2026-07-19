# rskit security model

This document covers the current Rust-side security posture for `rskit-auth`, `rskit-authz`,
`rskit-security`, and `rskit-encryption`.

## Threat model

- Protect bearer tokens, API keys, passwords, cookies,
  and encryption keys from accidental disclosure, replay, and substitution.
- Default to deny when authorization data is missing, mismatched, or stale.
- Require explicit transport security choices so production deployments keep TLS/HSTS on.
- Favor public-key token verification paths for external trust boundaries.

## Authentication

### JWT

- Default signing algorithm: `RS256`
- Preferred algorithms: `RS256`, `ES256`, `EdDSA`
- `HS256` is supported only through `JwtConfig::hs256_internal(...)` for explicit internal use
- Verification is config-driven, never token-header-driven
- Required claims: `sub`, `iss`, `aud`, `exp`, `nbf`, `iat`
- Clock skew defaults to 30 seconds and is capped at 60 seconds
- `alg: none` and algorithm-confusion attempts are rejected

### OIDC

- Discovery uses `/.well-known/openid-configuration`
- Public clients require authorization-code flow plus `PKCE` (`S256`)
- Exact redirect URIs are configured up front; wildcard redirects are not supported
- Authorization state and ID-token nonce are both enforced
- ID tokens validate against cached JWKS and refresh once on cache miss / key rotation
- Access tokens are sent to `userinfo` only in the `Authorization: Bearer` header

### Passwords and API keys

- Password hashing uses Argon2id with `m=65536`, `t=3`, `p=4`
- API keys are stored as peppered HMAC-SHA-256 digests
  and validated by prefix lookup plus constant-time compare
- Plaintext secrets are treated as transient values only

## Authorization

- `rskit-authz` provides RBAC + ABAC composition with role hierarchy
- Evaluation is deny-first and deny-by-default
- ABAC deny overrides RBAC or ABAC allow
- No policy match means access is denied

## HTTP security

- `rskit-http` provides secure-by-default HTTP response headers:
  - `Strict-Transport-Security` (HTTPS mode)
  - `Content-Security-Policy`
  - `X-Content-Type-Options`
  - `X-Frame-Options`
  - `Referrer-Policy`
  - `Permissions-Policy`
- Local insecure development mode omits HSTS while retaining the remaining headers
- `rskit-http` owns CORS policy because CORS is specific to HTTP/browser clients.

## Transport security

- `rskit-security` owns shared TLS configuration for transports.

## Encryption

- `rskit-encryption` remains the symmetric-crypto module
- Use it for payload/data protection, not token signing policy
