# rskit-inference-tgi

Hugging Face TGI REST adapter skeleton for `rskit-inference`.

Implementation pending; PRs welcome. This crate intentionally provides only the
explicit `register(&mut Registry)` factory wiring, descriptor, and trait shape.
It performs no auto-registration and has no global registry.
