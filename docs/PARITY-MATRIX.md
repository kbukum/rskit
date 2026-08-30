# Cross-Kit Parity Matrix — rskit

This matrix records the rskit side of reusable infrastructure and pattern parity. The module-presence table is kept identical to gokit's counterpart at https://github.com/kbukum/gokit/blob/main/docs/PARITY-MATRIX.md; the capability tables below are rskit-specific.

## Module presence & naming (shared cross-kit)

Legend: ✅ present · ➖ absent.

| Layer | Canonical concept | gokit | rskit | Note |
|---|---|---|---|---|
| L0 | errors | ✅ | ✅ | aligned |
| L0 | util | ✅ | ✅ | rskit `util` broader (time/env/hash/template/backoff) |
| L0 | version | ✅ | ✅ | build-info derived, immutable |
| L0 | codec | ✅ | ✅ | framing/value/json/toml |
| L0 | fs | ✅ | ✅ | safe paths, temp, atomic writes, permissions; watch is rskit-only |
| L1 | config | ✅ | ✅ | depends on `logging` for `LoggingConfig` |
| L1 | logging | ✅ | ✅ | naming aligned (was gokit `logger`) |
| L1 | validation | ✅ | ✅ | generic `Validate` seam |
| L1 | encryption | ✅ | ✅ | AES-GCM / ChaCha20 |
| L1 | schema | ✅ | ✅ | generics + compiled validator + limits |
| L2 | component / hook / provider / di | ✅ | ✅ | aligned lifecycle + 4 provider shapes |
| L3 | observability / resilience / security | ✅ | ✅ | aligned |
| L4 | bootstrap | ✅ | ✅ | aligned |
| L4 | stream | ✅ | ✅ | aligned: shared operator vocab (map/filter/fan-out/window/batch/parallel) + broadcaster + bounded backpressure (G4) |
| L4 | dag / chain / worker / process / stateful | ✅ | ✅ | aligned: `chain` is typed `Step[I,O]`/`Executor` cross-kit (no `any`) |
| L5 | server / httpclient / grpc / sse / discovery | ✅ | ✅ | aligned |
| L5 | http | ➖ | ✅ | intentional rskit-only: Axum-specific HTTP transport helper; gokit folds equivalent concerns into `server` (gin) |
| L5 | connect | ✅ | ➖ | intentional gokit-only: ConnectRPC (connect-go) adapter; rskit uses tonic/axum for the same role |
| L6 | auth / authz | ✅ | ✅ | aligned |
| L6 | database (+ sqlite) | ✅ | ✅ | aligned; both kits have a sqlite adapter |
| L6 | cache (+ redis) | ✅ | ✅ | aligned |
| L6 | storage (+ s3/gcs/supabase) | ✅ | ✅ | aligned; both have s3/gcs/supabase; gokit adds a local-fs adapter, rskit keeps the local default in core |
| L6 | vectorstore (+ qdrant) | ✅ | ✅ | aligned; both kits have memory default + qdrant adapter |
| L6 | messaging (kafka/nats/rabbitmq/memory) | ✅ | ✅ | aligned |
| L7 | ai / llm / embedding / inference | ✅ | ✅ | provider granularity differs (subdirs vs crates) |
| L7 | agent / tool / mcp / skill | ✅ | ✅ | MCP is a protocol-shaped module |
| L8 | media | ✅ | ✅ | gokit is a light **standalone module**: detection + metadata + cheap image ops + time/spatial + subtitle (SRT/VTT); heavy audio/video/matrix transcoding stays rskit by design |
| L9 | bench / git / testutil | ✅ | ✅ | module presence aligned; bench result JSON is a shared cross-kit contract — both kits emit `SCHEMA_VERSION` `"1.0"` / `SCHEMA_URL` `https://gokit.dev/bench/v1/schema.json`, the snake_case metric `direction` enum (no `descriptive` bool), and one envelope shape (run identity flat at top level, `branches` a sorted map keyed by name, string-keyed maps serialized in sorted order for deterministic output, empty `branches`/`curves`/`samples`/`tag` omitted); report formats are at parity (csv/json/junit/markdown/table/vegalite/html, with a self-contained Vega-embedding HTML reporter in both kits); rskit currently leads on CLI-backed signed annotated tags (`RefManager::create_signed_tag`), so sibling parity needs a follow-up capability check |
| L9 | workload | ✅ | ✅ | aligned: provider-based `Manager` + registry + component; backends stay in adapter crates |
| L9 | cli | ✅ | ✅ | aligned (light): theme/render/progress/prompt/signal + bounded live console; line + scripted prompt terminals with non-interactive fallback; raw-mode rich widgets stay rskit-only |
| L9 | dataset | ✅ | ✅ | cross-kit (light): one generic `Collector[T]` engine (bounded worker pool + `StreamBuffer` backpressure, per-source timeout/cancel, offset resume, real/AI stats, pluggable `Validator[T]`) over generic `Source`/`Transform`/`Target`; concrete item families for tabular `record.Record` (CSV/JSON-array/JSON-lines readers+writers, schema validator adapter) and blob `sample.Item` (labeled/offset, real/AI local disk target); manifest cache with one canonical `CacheStatusFor`. Deliberate divergences: gokit folds rskit's per-item `ItemSink<T>` and post-hoc directory `Target` into a single `stage.Target[T]` that publishes per source from the single-owner main loop (no shared sink); rskit's `MediaType`, rich `DataItem` metadata, and image/resize transforms stay rskit-only. pykit tracked separately |

## Infrastructure and pattern parity

| Concept | rskit shape | Cross-kit target | Status |
|---------|-------------|------------------|-------------|
| Provider shapes | `RequestResponse`, `Stream`, `Sink`, `Duplex` traits in `rskit-provider` | Same four concepts in every kit, idiomatic generics/protocols/traits | Aligned |
| Component lifecycle | `Component` trait with `start`, `stop`, `health` and registry ordering | Same lifecycle semantics and state vocabulary cross-kit | Aligned |
| Hook contracts | `rskit-hook` registry with typed event registration and emit/clear behavior | Same hook concept and ordering semantics cross-kit | Aligned |
| DI registration | `rskit-di` typed registration/resolve API | Typed registration/resolve, no service locator | Verify implementation parity during sibling alignment |
| Resilience policies | `RetryPolicy`, circuit breaker, bulkhead, rate limiter, timeout concepts in `rskit-resilience` | Same policy names and composition order cross-kit | Aligned |
| Config/source/format handling | Serde-based typed configs; domain helpers in `server` and `file` | Config loading/source precedence/masking centralized per kit | Align generic source/format behavior to `config`/`schema`/`storage` owners |
| Registry/backend selection | `Registry`/`Binding` in provider; component registry in component | Explicit injected registries, config-driven selection, no globals | Naming aligned; selector semantics require cross-kit parity before adapters share the pattern |
| Process execution | `rskit-process` owns timeout/env/capture/process-group behavior | Subprocess execution only through `process` | Aligned |
| Data and storage modules | `rskit-database`, `rskit-cache`, `rskit-storage`, `rskit-storage-s3`, `rskit-storage-gcs`, `rskit-vectorstore` | Abstraction crates with opt-in backend crates/features and explicit registration | Aligned; storage core remains lean while S3/GCS live in adapter crates |
| Messaging | `rskit-messaging` core owns broker-neutral `BrokerConfig`, `MessagingRegistry`, memory default, middleware, and DLQ envelope; Kafka/NATS/RabbitMQ are opt-in adapter crates/features that register typed factories and keep SDK dependencies out of core | Transport-agnostic producer/consumer + injected registry + memory default + opt-in broker adapters | Aligned — typed config capture at registration where needed, secure-by-default adapters with explicit insecure-dev opt-ins, canonical DLQ vocabulary, and canonical resilience reuse |

## AI / ML and agent surface parity

| Concept | rskit shape | Cross-kit target | Status |
|---------|-------------|------------------|--------|
| LLM core | `rskit-llm` owns provider traits, capabilities, stream events, and message/tool-call types | Same public concepts across kits with provider-specific dialects hidden behind adapters | Align finish/state semantics across kits |
| LLM providers | `rskit-llm-providers` has OpenAI, Anthropic, Gemini, and Ollama modules | OpenAI, Anthropic, Gemini, and Ollama in every kit; feature-gated adapters with explicit registration | Aligned; gokit `llm/providers` mirrors the same four providers |
| Agent loop | `rskit-agent` has turn loop, hooks, token budget, memory/context strategy types | Bounded turns, wall-clock budget, token budget, cancellation propagation, backpressure, and identical hook/event semantics | Enhance: thread canonical deadline/cancel/budget through provider and tool calls; add streaming loop parity |
| Tool definitions | `rskit-tool` owns callable, registry, definition, annotations, middleware, retry, metrics | Typed tools with JSON Schema input/output, structured results, MCP annotations, explicit registry ownership | Align: move reusable retry/timeout/metrics/logging/validation/policy ownership to canonical crates |
| MCP | `rskit-mcp` exposes tool list/call server and remote tool discovery/wrapping | Protocol-shaped tools, prompts, resources/templates, roots, sampling, elicitation, cancellation, progress, logging, stdio, Streamable HTTP | Redesign from tool bridge to protocol-shaped module |
| MCP security | Protocol endpoints and remote-tool access are security boundaries | Allow-list, authz, audit, payload/result limits, output validation, Origin validation, local bind defaults, HTTP auth | Enhance with `rskit-authz`, `rskit-security`, `rskit-observability`, `rskit-resilience` |
| Schema | `rskit-schema` owns generation and validation | Schema owner for tool input/output, MCP prompts/resources/elicitation, structured LLM output, and inference APIs | Leave as owner; enhance for output/structured-content validation where needed |
| Embedding | `rskit-embedding` owns SDK-free embedding contracts and an in-memory provider | Provider abstraction, batch embeddings, dimensions, normalization, and endpoint ownership aligned with `llm-providers`/`inference` | Align endpoint ownership with sibling kits |
| Inference | `rskit-inference` is an independent module with neutral contracts, explicit registry/building, and an `openai_compatible` adapter | Cross-kit inference module with registry/config-selected backends and embeddings where supported | Align/Enhance backend set and richer parity from the neutral module base |
| Agent Skills packages | `rskit-mcp` now exposes lightweight skill-pack types plus `kit.skill.json`/`SKILL.md` discovery and loading | Lightweight Agent Skills-compatible discovery/loader over tools/prompts/resources/MCP; no custom runtime | Aligned thin discovery/runtime split |
| Token counting | `rskit-llm` owns the pluggable `TokenCounter` port + dependency-free heuristic default; exact tokenizers ship as opt-in `contrib/llm/tiktoken` and `contrib/llm/hf-tokenizers` adapters; `rskit-bench` exposes an injected `token_stats` metric | Deterministic token-counting port + heuristic default in core, exact tokenizer adapters opt-in, and a benchmark token metric in every kit | Tracked in gokit (https://github.com/kbukum/gokit/issues/341) and pykit (https://github.com/kbukum/pykit/issues/143) |
| Async / semantic-similarity metrics | `rskit-bench` owns the `AsyncMetric` port (I/O-backed scoring awaited after synchronous metrics) and `semantic_similarity`, an embedding-cosine metric over an injected `rskit-embedding` provider, with a comparison-safe result name that folds in both the embedding-model identity and the configured threshold, and canonical `rskit-resilience` timeout/retry; every threshold-dependent metric (`classification`, `fuzzy_match`, `semantic_similarity`, `llm_judge`) folds its cutoff into the metric identity through one shared canonical formatter that normalizes `-0` to `0`, validates it as finite and in `[0, 1]` (typed `InvalidInput` otherwise), and records it as provenance rather than a directional value, so runs scored at different thresholds are never joined and diffed | Async, I/O-backed benchmark-metric port plus an embedding-similarity metric in every kit, with threshold folded into metric identity via one canonical formatter so runs at different cutoffs are never compared as equivalent | `AsyncMetric` + `semantic_similarity` tracked in gokit (https://github.com/kbukum/gokit/issues/342) and pykit (https://github.com/kbukum/pykit/issues/144); the shared canonical threshold formatter and threshold-in-identity comparison-safety across `classification`/`fuzzy_match`/`semantic_similarity`/`llm_judge` is an rskit hardening whose sibling propagation is not yet tracked in a dedicated issue |
| LLM-judge metric | `rskit-bench` owns `llm_judge`, an `AsyncMetric` grading predictions via an injected `rskit-llm` provider and a versioned `JudgePrompt` (canonical `rskit-ai` prompt template with semver identity), parsing the untrusted reply into a validated `JudgeVerdict` (malformed/out-of-range/non-JSON replies, over-long replies, and incomplete completions → typed error, no success-shaped fallback), with canonical `rskit-resilience` timeout and bounded concurrency; judge provider/model + prompt identity recorded in `RunProvenance`. Input and trust-boundary hardening: a retry-configured or timeout-less resilience policy and a blank (trimmed) model are rejected at construction, the rendered prompt is bounded by a configurable `max_prompt_bytes` (default 128 KiB), duplicate top-level `score`/`rationale` keys in the reply are rejected (case-folded), reply-shape faults are typed as `ExternalService`, and model resolution is strictly all-or-nothing. Comparison safety: `RunComparator` detects incompatible judges (a judge metric whose resolved backend model differs across the two runs), records the incompatibility, and excludes that metric and its `<metric>.<key>` subvalues from regression classification | Async LLM-judge benchmark metric with a versioned prompt, structured untrusted-output parsing, input/trust-boundary hardening, and incompatible-judge detection in run comparison, in every kit | Tracked in gokit (https://github.com/kbukum/gokit/issues/340) and pykit (https://github.com/kbukum/pykit/issues/142) |
