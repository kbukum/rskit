# rskit-vectorstore-qdrant

Qdrant adapter for `rskit-vectorstore`. Register it explicitly with a `VectorStoreRegistry`.
Use `rskit_config::SecretString` for API keys; URLs with embedded credentials or query
parameters are rejected to avoid leaking connection details in diagnostics.
