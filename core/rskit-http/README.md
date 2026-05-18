# rskit-http

Shared HTTP transport policies and Tower adapters for rskit.

`rskit-http` no longer owns the service-facing server abstraction. Use `rskit-server` for
`HttpServerBuilder`, health routes, and interceptor ordering.

This crate keeps reusable HTTP transport primitives:

- `RequestId` / `CorrelationId` toolkit-native extension helpers
- `HttpRequest` / `HttpResponse` / `HttpHeaders` protocol types from the framework-neutral `http` crate
- `status_to_error_code` and `app_error_status` status mapping helpers
- `CorsPolicy`
- `SecurityHeadersConfig` / `SecurityHeadersLayer`
- tenant extension helpers

Security header policy and cross-transport TLS settings are owned by
`rskit-security`. `rskit-http` only supplies HTTP/Tower adapters. Health routes,
server defaults, and service interceptor ordering remain in `rskit-server`.
