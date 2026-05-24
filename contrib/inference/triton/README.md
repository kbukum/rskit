# rskit-inference-triton

Triton KServe v2 HTTP adapter for `rskit-inference`.

Implemented:

- `Inference::predict` against `/v2/models/{name}/infer`
- versioned path `/v2/models/{name}/versions/{version}/infer`
- component health reports registration/configuration availability; readiness probing should be owned by
  the service that hosts the adapter
- FP32, INT64, and BYTES tensor encode/decode
- explicit `register(&mut Registry, Config)` wiring with optional `rskit-resilience::Policy`
  and `rskit-authz::Decider` configured on `Config`
- OTel GenAI semantic-convention attributes via `tracing` spans

At encode time, `PredictRequest::options` is merged into the KServe `parameters` object.
When the same key appears in both `options` and `parameters`, `parameters` wins because it is
the explicit typed field.

KServe v2 HTTP has no native streaming protocol, so this crate does not implement
`StreamingInference`.
