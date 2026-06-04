#![no_main]

use libfuzzer_sys::fuzz_target;
use rskit_schema::{ValidationLimits, ValidationOptions};
use serde_json::Value;

const LIMITS: ValidationLimits = ValidationLimits::new(16, 512)
    .with_max_string_bytes(4096)
    .with_max_key_bytes(512)
    .with_max_total_string_bytes(16_384);

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<Value>(data) else {
        return;
    };

    let schema = serde_json::json!({});
    let options = ValidationOptions { limits: LIMITS };

    let _ = rskit_schema::compile_with_options(&value, options);
    let _ = rskit_schema::validate_with_options(&schema, &value, options);
    let _ = rskit_schema::validate(&value, &serde_json::json!(null));
});
