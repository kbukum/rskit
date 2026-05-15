# rskit-http

Axum transport details for rskit.

`rskit-http` no longer owns the service-facing server abstraction. Use `rskit-server` for
`HttpServerBuilder`, health routes, and interceptor ordering.

This crate keeps reusable HTTP transport primitives:

- `HttpError`
- `ErrorHandlerLayer`
- `RequestId` / `CorrelationId`
- `CorsPolicy`
- `SecurityHeadersConfig` / `SecurityHeadersLayer`
- tenant extraction helpers and middleware

Cross-transport TLS settings remain in `rskit-security`. Health routes and service
interceptor ordering remain in `rskit-server`.
