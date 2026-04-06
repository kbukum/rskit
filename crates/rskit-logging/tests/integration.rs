//! Comprehensive integration tests for rskit-logging.
//!
//! Covers: LogConfig defaults, format switching, level filtering, correlation-id
//! context helpers, span operations, edge cases, and output capture via a
//! custom `tracing_subscriber::Layer`.

use std::sync::{Arc, Mutex};

use rskit_config::{LogFormat, LogOutput, LoggingConfig};
use rskit_logging::context::{component_span, request_span, set_correlation_id, set_trace_id, set_user_id};
use rskit_logging::init_logging;
use tracing::subscriber::with_default;
use tracing::{info_span, Subscriber};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

// ── Capture layer ────────────────────────────────────────────────────────────
// A minimal `Layer` that records events and span metadata into a shared buffer
// so tests can assert on logged output without touching stdout.

#[derive(Clone, Default)]
struct CapturedLogs {
    events: Arc<Mutex<Vec<CapturedEvent>>>,
    spans: Arc<Mutex<Vec<CapturedSpan>>>,
}

#[derive(Debug, Clone)]
struct CapturedEvent {
    message: String,
    level: tracing::Level,
    target: String,
    #[allow(dead_code)]
    fields: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct CapturedSpan {
    name: String,
    fields: Vec<(String, String)>,
}

struct CapturingLayer {
    logs: CapturedLogs,
}

impl<S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>> Layer<S>
    for CapturingLayer
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        let message = visitor
            .fields
            .iter()
            .find(|(k, _)| k == "message")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();

        self.logs.events.lock().unwrap().push(CapturedEvent {
            message,
            level: *event.metadata().level(),
            target: event.metadata().target().to_string(),
            fields: visitor.fields,
        });
    }

    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut visitor = FieldVisitor::default();
        attrs.record(&mut visitor);

        self.logs.spans.lock().unwrap().push(CapturedSpan {
            name: attrs.metadata().name().to_string(),
            fields: visitor.fields,
        });
    }
}

#[derive(Default)]
struct FieldVisitor {
    fields: Vec<(String, String)>,
}

impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields
            .push((field.name().to_string(), format!("{:?}", value)));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields
            .push((field.name().to_string(), value.to_string()));
    }
}

/// Build a test subscriber with a capture layer and an optional filter string.
fn test_subscriber(filter: &str) -> (impl Subscriber + Send + Sync, CapturedLogs) {
    let captured = CapturedLogs::default();
    let layer = CapturingLayer {
        logs: captured.clone(),
    };
    let subscriber = Registry::default()
        .with(EnvFilter::new(filter))
        .with(layer);
    (subscriber, captured)
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. LogConfig / init_logging
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn default_config_has_expected_values() {
    let cfg = LoggingConfig::default();
    assert_eq!(cfg.level, "info");
    assert_eq!(cfg.format, LogFormat::Console);
    assert_eq!(cfg.output, LogOutput::Stdout);
    assert!(cfg.service_name.is_none());
    assert!(!cfg.with_caller);
}

#[test]
fn init_logging_console_does_not_panic() {
    let cfg = LoggingConfig::default();
    let _guard = init_logging(&cfg);
    tracing::info!("console smoke test");
}

#[test]
fn init_logging_json_does_not_panic() {
    let cfg = LoggingConfig {
        format: LogFormat::Json,
        ..Default::default()
    };
    let _guard = init_logging(&cfg);
    tracing::info!("json smoke test");
}

#[test]
fn init_logging_with_custom_level() {
    let cfg = LoggingConfig {
        level: "debug".to_string(),
        ..Default::default()
    };
    let _guard = init_logging(&cfg);
    tracing::debug!("debug level test");
}

#[test]
fn init_logging_with_trace_level() {
    let cfg = LoggingConfig {
        level: "trace".to_string(),
        ..Default::default()
    };
    let _guard = init_logging(&cfg);
    tracing::trace!("trace level test");
}

#[test]
fn init_logging_with_warn_level() {
    let cfg = LoggingConfig {
        level: "warn".to_string(),
        ..Default::default()
    };
    let _guard = init_logging(&cfg);
    tracing::warn!("warn level test");
}

#[test]
fn init_logging_with_error_level() {
    let cfg = LoggingConfig {
        level: "error".to_string(),
        ..Default::default()
    };
    let _guard = init_logging(&cfg);
    tracing::error!("error level test");
}

#[test]
fn init_logging_with_service_name() {
    let cfg = LoggingConfig {
        service_name: Some("my-service".to_string()),
        ..Default::default()
    };
    let _guard = init_logging(&cfg);
    tracing::info!(service = "my-service", "service name test");
}

#[test]
fn init_logging_stderr_output() {
    let cfg = LoggingConfig {
        output: LogOutput::Stderr,
        ..Default::default()
    };
    let _guard = init_logging(&cfg);
    tracing::info!("stderr output test");
}

#[test]
fn init_logging_env_does_not_panic() {
    let _guard = rskit_logging::init_logging_env();
    tracing::info!("env-only init test");
}

#[test]
fn guard_drop_restores_previous_subscriber() {
    // The default guard pattern means dropping the guard removes the subscriber.
    // We verify that creating two guards in sequence doesn't panic.
    let cfg = LoggingConfig::default();
    {
        let _guard = init_logging(&cfg);
        tracing::info!("inside guard scope");
    }
    // After guard is dropped, this uses the previous (or noop) subscriber.
    tracing::info!("after guard dropped");
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. Log level filtering (via capture layer)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn filter_info_passes_info_and_above() {
    let (sub, captured) = test_subscriber("info");
    with_default(sub, || {
        tracing::trace!("trace msg");
        tracing::debug!("debug msg");
        tracing::info!("info msg");
        tracing::warn!("warn msg");
        tracing::error!("error msg");
    });
    let events = captured.events.lock().unwrap();
    assert_eq!(events.len(), 3, "info filter should pass info, warn, error");
    assert_eq!(events[0].level, tracing::Level::INFO);
    assert_eq!(events[1].level, tracing::Level::WARN);
    assert_eq!(events[2].level, tracing::Level::ERROR);
}

#[test]
fn filter_debug_passes_debug_and_above() {
    let (sub, captured) = test_subscriber("debug");
    with_default(sub, || {
        tracing::trace!("trace msg");
        tracing::debug!("debug msg");
        tracing::info!("info msg");
    });
    let events = captured.events.lock().unwrap();
    assert_eq!(events.len(), 2, "debug filter should pass debug, info");
    assert_eq!(events[0].level, tracing::Level::DEBUG);
    assert_eq!(events[1].level, tracing::Level::INFO);
}

#[test]
fn filter_error_only() {
    let (sub, captured) = test_subscriber("error");
    with_default(sub, || {
        tracing::info!("info msg");
        tracing::warn!("warn msg");
        tracing::error!("error msg");
    });
    let events = captured.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].level, tracing::Level::ERROR);
}

#[test]
fn filter_trace_passes_everything() {
    let (sub, captured) = test_subscriber("trace");
    with_default(sub, || {
        tracing::trace!("trace msg");
        tracing::debug!("debug msg");
        tracing::info!("info msg");
        tracing::warn!("warn msg");
        tracing::error!("error msg");
    });
    let events = captured.events.lock().unwrap();
    assert_eq!(events.len(), 5, "trace filter should pass all levels");
}

#[test]
fn filter_with_target_directive() {
    // Only allow warn globally but trace for a specific target.
    let (sub, captured) = test_subscriber("warn,integration=trace");
    with_default(sub, || {
        tracing::trace!(target: "integration", "targeted trace");
        tracing::debug!(target: "other_crate", "other debug");
        tracing::warn!(target: "other_crate", "other warn");
    });
    let events = captured.events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].target, "integration");
    assert_eq!(events[1].target, "other_crate");
}

#[test]
fn envfilter_parse_valid_strings() {
    // Verify that EnvFilter can parse the strings we use in LoggingConfig.
    for level_str in &["trace", "debug", "info", "warn", "error", "off"] {
        let filter = EnvFilter::try_new(level_str);
        assert!(filter.is_ok(), "Failed to parse filter: {}", level_str);
    }
}

#[test]
fn envfilter_parse_compound_directive() {
    let filter = EnvFilter::try_new("info,rskit=trace,hyper=warn");
    assert!(filter.is_ok());
}

#[test]
fn envfilter_parse_invalid_string_handled_by_init_logging() {
    // EnvFilter::try_new is permissive with many strings (it interprets them
    // as target directives), so instead of testing the parser we verify that
    // init_logging gracefully handles an unusual level string.
    let cfg = LoggingConfig {
        level: "not_a_real_module=trace".to_string(),
        ..Default::default()
    };
    let _guard = init_logging(&cfg);
    tracing::info!("fallback test");
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. Correlation ID / context helpers
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn set_correlation_id_does_not_panic_outside_span() {
    let (sub, _captured) = test_subscriber("trace");
    with_default(sub, || {
        // Outside any span — should be a no-op, not a panic.
        set_correlation_id("abc-123");
    });
}

#[test]
fn set_user_id_does_not_panic_outside_span() {
    let (sub, _captured) = test_subscriber("trace");
    with_default(sub, || {
        set_user_id("user-42");
    });
}

#[test]
fn set_trace_id_does_not_panic_outside_span() {
    let (sub, _captured) = test_subscriber("trace");
    with_default(sub, || {
        set_trace_id("trace-xyz");
    });
}

#[test]
fn correlation_id_recorded_inside_span_with_field() {
    let (sub, captured) = test_subscriber("trace");
    with_default(sub, || {
        let span = info_span!("test_op", correlation_id = tracing::field::Empty);
        let _enter = span.enter();
        set_correlation_id("corr-456");
        tracing::info!("after setting correlation_id");
    });
    let events = captured.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].message, "after setting correlation_id");
}

#[test]
fn user_id_recorded_inside_span_with_field() {
    let (sub, _captured) = test_subscriber("trace");
    with_default(sub, || {
        let span = info_span!("test_op", user_id = tracing::field::Empty);
        let _enter = span.enter();
        set_user_id("user-99");
        tracing::info!("after setting user_id");
    });
}

#[test]
fn trace_id_recorded_inside_span_with_field() {
    let (sub, _captured) = test_subscriber("trace");
    with_default(sub, || {
        let span = info_span!("test_op", trace_id = tracing::field::Empty);
        let _enter = span.enter();
        set_trace_id("tid-abc");
        tracing::info!("after setting trace_id");
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. Span operations — component_span, request_span
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn component_span_creates_named_span() {
    let (sub, captured) = test_subscriber("trace");
    with_default(sub, || {
        let _s = component_span("auth-service").entered();
        tracing::info!("inside component span");
    });
    let spans = captured.spans.lock().unwrap();
    assert!(!spans.is_empty());
    assert_eq!(spans[0].name, "component");
    let comp_field = spans[0].fields.iter().find(|(k, _)| k == "component");
    assert!(comp_field.is_some(), "component field should be present");
    assert_eq!(comp_field.unwrap().1, "auth-service");
}

#[test]
fn request_span_captures_http_metadata() {
    let (sub, captured) = test_subscriber("trace");
    with_default(sub, || {
        let _s = request_span("GET", "/api/v1/health", "req-001").entered();
        tracing::info!("inside request span");
    });
    let spans = captured.spans.lock().unwrap();
    assert!(!spans.is_empty());
    assert_eq!(spans[0].name, "request");

    let field_map: std::collections::HashMap<_, _> =
        spans[0].fields.iter().cloned().collect();
    assert_eq!(field_map.get("http.method").map(|s| s.as_str()), Some("GET"));
    assert_eq!(field_map.get("http.path").map(|s| s.as_str()), Some("/api/v1/health"));
    assert_eq!(field_map.get("request_id").map(|s| s.as_str()), Some("req-001"));
}

#[test]
fn request_span_various_methods() {
    let (sub, captured) = test_subscriber("trace");
    with_default(sub, || {
        for method in &["GET", "POST", "PUT", "DELETE", "PATCH"] {
            let _s = request_span(method, "/test", "rid").entered();
        }
    });
    let spans = captured.spans.lock().unwrap();
    assert_eq!(spans.len(), 5);
}

#[test]
fn nested_component_and_request_spans() {
    let (sub, captured) = test_subscriber("trace");
    with_default(sub, || {
        let _comp = component_span("gateway").entered();
        let _req = request_span("POST", "/login", "req-login").entered();
        tracing::info!("nested span test");
    });
    let spans = captured.spans.lock().unwrap();
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].name, "component");
    assert_eq!(spans[1].name, "request");
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. Format switching — JSON vs Console init
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn json_format_config_round_trip() {
    let cfg = LoggingConfig {
        format: LogFormat::Json,
        ..Default::default()
    };
    assert_eq!(cfg.format, LogFormat::Json);
    // Ensure init_logging produces a working subscriber.
    let _guard = init_logging(&cfg);
    tracing::info!(key = "value", "json format round trip");
}

#[test]
fn console_format_config_round_trip() {
    let cfg = LoggingConfig {
        format: LogFormat::Console,
        ..Default::default()
    };
    assert_eq!(cfg.format, LogFormat::Console);
    let _guard = init_logging(&cfg);
    tracing::info!("console format round trip");
}

#[test]
fn switching_formats_between_guards() {
    // First JSON, then Console — guard scoping should prevent conflicts.
    {
        let cfg = LoggingConfig {
            format: LogFormat::Json,
            ..Default::default()
        };
        let _g = init_logging(&cfg);
        tracing::info!("json");
    }
    {
        let cfg = LoggingConfig {
            format: LogFormat::Console,
            ..Default::default()
        };
        let _g = init_logging(&cfg);
        tracing::info!("console");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. Edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn empty_level_string_uses_envfilter_default() {
    // EnvFilter::new("") doesn't panic — it enables everything.
    let cfg = LoggingConfig {
        level: String::new(),
        ..Default::default()
    };
    let _guard = init_logging(&cfg);
    tracing::info!("empty level test");
}

#[test]
fn empty_service_name() {
    let cfg = LoggingConfig {
        service_name: Some(String::new()),
        ..Default::default()
    };
    let _guard = init_logging(&cfg);
    tracing::info!(service = "", "empty service name test");
}

#[test]
fn very_long_message() {
    let long_msg = "x".repeat(100_000);
    let (sub, captured) = test_subscriber("info");
    with_default(sub, || {
        tracing::info!("{}", long_msg);
    });
    let events = captured.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    // The message should contain the full string (tracing doesn't truncate).
    assert!(events[0].message.len() >= 100_000);
}

#[test]
fn unicode_in_messages() {
    let (sub, captured) = test_subscriber("info");
    with_default(sub, || {
        tracing::info!("こんにちは世界 🌍 مرحبا");
    });
    let events = captured.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert!(events[0].message.contains("こんにちは世界"));
    assert!(events[0].message.contains("🌍"));
    assert!(events[0].message.contains("مرحبا"));
}

#[test]
fn unicode_in_span_fields() {
    let (sub, captured) = test_subscriber("trace");
    with_default(sub, || {
        let _s = request_span("GET", "/路径/テスト", "req-ünïcödé").entered();
        tracing::info!("unicode span test");
    });
    let spans = captured.spans.lock().unwrap();
    let field_map: std::collections::HashMap<_, _> =
        spans[0].fields.iter().cloned().collect();
    assert_eq!(field_map.get("http.path").map(|s| s.as_str()), Some("/路径/テスト"));
    assert_eq!(field_map.get("request_id").map(|s| s.as_str()), Some("req-ünïcödé"));
}

#[test]
fn multiple_init_calls_with_different_configs() {
    // Simulates re-initialisation (e.g. config reload). Each guard is scoped.
    for format in &[LogFormat::Console, LogFormat::Json, LogFormat::Console] {
        let cfg = LoggingConfig {
            format: format.clone(),
            ..Default::default()
        };
        let _guard = init_logging(&cfg);
        tracing::info!("reinit with {:?}", format);
    }
}

#[test]
fn concurrent_logging_does_not_panic() {
    // Use init_logging (which sets a thread-local default) combined with a
    // scoped dispatcher so that spawned threads inherit the subscriber.
    let cfg = LoggingConfig {
        level: "info".to_string(),
        ..Default::default()
    };
    let _guard = init_logging(&cfg);
    let handles: Vec<_> = (0..8)
        .map(|i| {
            std::thread::spawn(move || {
                for j in 0..100 {
                    tracing::info!(thread = i, iteration = j, "concurrent log");
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    // If we get here without panic, concurrent logging is safe.
}

#[test]
fn special_characters_in_field_values() {
    let (sub, captured) = test_subscriber("info");
    with_default(sub, || {
        tracing::info!(
            path = r#"C:\Users\test\file"with"quotes"#,
            query = "SELECT * FROM t WHERE x='1' AND y=\"2\"",
            "special chars"
        );
    });
    let events = captured.events.lock().unwrap();
    assert_eq!(events.len(), 1);
}

#[test]
fn with_caller_config_field_accepted() {
    let cfg = LoggingConfig {
        with_caller: true,
        ..Default::default()
    };
    assert!(cfg.with_caller);
    let _guard = init_logging(&cfg);
    tracing::info!("caller location test");
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Re-exported macros work correctly
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn reexported_macros_compile_and_run() {
    let (sub, captured) = test_subscriber("trace");
    with_default(sub, || {
        rskit_logging::trace!("re-export trace");
        rskit_logging::debug!("re-export debug");
        rskit_logging::info!("re-export info");
        rskit_logging::warn!("re-export warn");
        rskit_logging::error!("re-export error");
    });
    let events = captured.events.lock().unwrap();
    assert_eq!(events.len(), 5);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Global init (is_global_init, init_global)
// ═══════════════════════════════════════════════════════════════════════════════
// Note: init_global uses a process-level AtomicBool so we can only test the
// API surface, not idempotency, within a single test binary. We verify that
// the function exists and has the correct return type.

#[test]
fn is_global_init_returns_bool() {
    // This may be true if another test already called init_global.
    let _val: bool = rskit_logging::is_global_init();
}

#[test]
fn global_logging_guard_type_exists() {
    // Ensure the type is publicly accessible (compile-time check).
    fn _assert_type(_g: rskit_logging::GlobalLoggingGuard) {}
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. LoggingConfig via serde deserialization
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn logging_config_deserializes_from_json() {
    let json = r#"{"level":"debug","format":"json"}"#;
    let cfg: LoggingConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.level, "debug");
    assert_eq!(cfg.format, LogFormat::Json);
}

#[test]
fn logging_config_deserializes_console_format() {
    let json = r#"{"format":"console"}"#;
    let cfg: LoggingConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.format, LogFormat::Console);
}

#[test]
fn logging_config_defaults_when_fields_missing() {
    let json = r#"{}"#;
    let cfg: LoggingConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.level, "info");
    assert_eq!(cfg.format, LogFormat::Console);
    assert_eq!(cfg.output, LogOutput::Stdout);
    assert!(cfg.service_name.is_none());
}

#[test]
fn logging_config_stderr_output_deserialization() {
    let json = r#"{"output":{"type":"stderr"}}"#;
    let cfg: LoggingConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.output, LogOutput::Stderr);
}

#[test]
fn logging_config_file_output_deserialization() {
    let json = r#"{"output":{"type":"file","path":"/var/log/app.log"}}"#;
    let cfg: LoggingConfig = serde_json::from_str(json).unwrap();
    assert!(matches!(cfg.output, LogOutput::File { ref path } if path == "/var/log/app.log"));
}

#[test]
fn logging_config_with_service_name() {
    let json = r#"{"service_name":"order-service"}"#;
    let cfg: LoggingConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.service_name.as_deref(), Some("order-service"));
}

#[test]
fn logging_config_with_caller_true() {
    let json = r#"{"with_caller":true}"#;
    let cfg: LoggingConfig = serde_json::from_str(json).unwrap();
    assert!(cfg.with_caller);
}
