# rskit-storage-supabase

Supabase Storage adapter for `rskit-storage`.

This opt-in crate uses the Supabase Storage REST API with bearer tokens in headers only. Importing it has no side effects; applications explicitly call `register` with an injected registry.
