# rskit-http

Axum transport details for rskit.

`rskit-http` no longer owns the service-facing server abstraction. Use `rskit-server` for
`HttpServerBuilder`, health routes, and interceptor ordering.

This crate keeps reusable HTTP transport primitives:

- `HttpError`
- `ErrorHandlerLayer`
- `RequestId` / `CorrelationId`
- tenant extraction helpers and middleware

TLS policy, health routes, and service interceptor ordering remain owned by `rskit-server`
so `rskit-http` stays transport-only.
