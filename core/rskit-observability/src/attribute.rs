//! Span attribute helpers shared by tracing and OpenTelemetry integrations.

use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Value that can be attached to an observability span attribute.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SpanAttributeValue {
    /// String attribute value.
    String(String),
    /// Signed integer attribute value.
    I64(i64),
    /// Floating-point attribute value.
    F64(f64),
    /// Boolean attribute value.
    Bool(bool),
}

impl From<&str> for SpanAttributeValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for SpanAttributeValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&String> for SpanAttributeValue {
    fn from(value: &String) -> Self {
        Self::String(value.clone())
    }
}

impl From<i64> for SpanAttributeValue {
    fn from(value: i64) -> Self {
        Self::I64(value)
    }
}

impl From<i32> for SpanAttributeValue {
    fn from(value: i32) -> Self {
        Self::I64(i64::from(value))
    }
}

impl From<u64> for SpanAttributeValue {
    fn from(value: u64) -> Self {
        match i64::try_from(value) {
            Ok(value) => Self::I64(value),
            Err(_) => Self::I64(i64::MAX),
        }
    }
}

impl From<usize> for SpanAttributeValue {
    fn from(value: usize) -> Self {
        match i64::try_from(value) {
            Ok(value) => Self::I64(value),
            Err(_) => Self::I64(i64::MAX),
        }
    }
}

impl From<f64> for SpanAttributeValue {
    fn from(value: f64) -> Self {
        Self::F64(value)
    }
}

impl From<f32> for SpanAttributeValue {
    fn from(value: f32) -> Self {
        Self::F64(f64::from(value))
    }
}

impl From<bool> for SpanAttributeValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

/// Set an OpenTelemetry span attribute on a tracing span.
pub fn set_span_attribute(span: &Span, key: &'static str, value: impl Into<SpanAttributeValue>) {
    set_opentelemetry_attribute(span, key, value.into());
}

/// Record a tracing span field and set the same OpenTelemetry span attribute.
///
/// `tracing` can only record fields declared when the span was created. If the field was not declared,
/// the tracing record is ignored while the OpenTelemetry attribute is still set.
pub fn record_span_attribute(span: &Span, key: &'static str, value: impl Into<SpanAttributeValue>) {
    let value = value.into();
    record_tracing_field(span, key, &value);
    set_opentelemetry_attribute(span, key, value);
}

/// Set an OpenTelemetry attribute on the current span.
pub fn set_current_span_attribute(key: &'static str, value: impl Into<SpanAttributeValue>) {
    set_span_attribute(&Span::current(), key, value);
}

/// Record a tracing field and set an OpenTelemetry attribute on the current span.
pub fn record_current_span_attribute(key: &'static str, value: impl Into<SpanAttributeValue>) {
    record_span_attribute(&Span::current(), key, value);
}

fn record_tracing_field(span: &Span, key: &'static str, value: &SpanAttributeValue) {
    match value {
        SpanAttributeValue::String(value) => span.record(key, value.as_str()),
        SpanAttributeValue::I64(value) => span.record(key, *value),
        SpanAttributeValue::F64(value) => span.record(key, *value),
        SpanAttributeValue::Bool(value) => span.record(key, *value),
    };
}

fn set_opentelemetry_attribute(span: &Span, key: &'static str, value: SpanAttributeValue) {
    match value {
        SpanAttributeValue::String(value) => span.set_attribute(key, value),
        SpanAttributeValue::I64(value) => span.set_attribute(key, value),
        SpanAttributeValue::F64(value) => span.set_attribute(key, value),
        SpanAttributeValue::Bool(value) => span.set_attribute(key, value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_values_saturate_to_otlp_integer_range() {
        assert_eq!(
            SpanAttributeValue::from(u64::MAX),
            SpanAttributeValue::I64(i64::MAX)
        );
        assert_eq!(SpanAttributeValue::from(7_u64), SpanAttributeValue::I64(7));
        assert_eq!(
            SpanAttributeValue::from(usize::MAX),
            SpanAttributeValue::I64(i64::MAX)
        );
        assert_eq!(
            SpanAttributeValue::from(7_usize),
            SpanAttributeValue::I64(7)
        );
    }

    #[test]
    fn helpers_accept_common_attribute_values() {
        let owned = String::from("owned");
        assert_eq!(
            SpanAttributeValue::from(owned.clone()),
            SpanAttributeValue::String(owned.clone())
        );
        assert_eq!(
            SpanAttributeValue::from(&owned),
            SpanAttributeValue::String(owned)
        );
        assert_eq!(SpanAttributeValue::from(7_i32), SpanAttributeValue::I64(7));
        assert_eq!(
            SpanAttributeValue::from(1.25_f32),
            SpanAttributeValue::F64(1.25)
        );

        let span = tracing::info_span!(
            "attribute-test",
            string = tracing::field::Empty,
            integer = tracing::field::Empty,
            float = tracing::field::Empty,
            boolean = tracing::field::Empty,
        );

        record_span_attribute(&span, "string", "value");
        record_span_attribute(&span, "integer", 42_i64);
        record_span_attribute(&span, "float", 1.5_f64);
        record_span_attribute(&span, "boolean", true);
        set_span_attribute(&span, "otel.only", "value");

        let _entered = span.enter();
        set_current_span_attribute("current.otel", false);
        record_current_span_attribute("boolean", false);
    }
}
