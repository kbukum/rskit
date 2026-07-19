//! Behavioral tests for the masking engine.

use std::borrow::Cow;
use std::sync::Arc;

use super::*;

fn default_masker() -> DefaultMasker {
    DefaultMasker::default()
}

// ── Field-name masking ──────────────────────────────────────────

#[test]
fn masks_password_field() {
    let m = default_masker();
    assert_eq!(m.mask_value("password", "hunter2").as_ref(), "[REDACTED]");
}

#[test]
fn masks_api_key_variants() {
    let m = default_masker();
    assert_eq!(m.mask_value("api_key", "sk-1234").as_ref(), "[REDACTED]");
    assert_eq!(m.mask_value("apikey", "sk-1234").as_ref(), "[REDACTED]");
    assert_eq!(m.mask_value("api-key", "sk-1234").as_ref(), "[REDACTED]");
}

#[test]
fn masks_token_fields() {
    let m = default_masker();
    for field in ["token", "access_token", "refresh_token", "auth_token"] {
        assert_eq!(
            m.mask_value(field, "abc123").as_ref(),
            "[REDACTED]",
            "field: {}",
            field
        );
    }
}

#[test]
fn masks_authorization_field() {
    let m = default_masker();
    assert_eq!(
        m.mask_value("authorization", "Bearer xyz").as_ref(),
        "[REDACTED]"
    );
}

#[test]
fn masks_private_key_field() {
    let m = default_masker();
    assert_eq!(
        m.mask_value("private_key", "-----BEGIN RSA").as_ref(),
        "[REDACTED]"
    );
}

#[test]
fn masks_ssn_field() {
    let m = default_masker();
    assert_eq!(m.mask_value("ssn", "123-45-6789").as_ref(), "[REDACTED]");
}

#[test]
fn masks_credit_card_field() {
    let m = default_masker();
    assert_eq!(
        m.mask_value("credit_card", "4111111111111111").as_ref(),
        "[REDACTED]"
    );
}

#[test]
fn masks_card_number_cvv_pin_secret_fields() {
    let m = default_masker();
    assert_eq!(m.mask_value("card_number", "x").as_ref(), "[REDACTED]");
    assert_eq!(m.mask_value("cvv", "123").as_ref(), "[REDACTED]");
    assert_eq!(m.mask_value("pin", "9876").as_ref(), "[REDACTED]");
    assert_eq!(m.mask_value("secret", "x").as_ref(), "[REDACTED]");
}

#[test]
fn does_not_mask_safe_field() {
    let m = default_masker();
    assert_eq!(m.mask_value("name", "Alice").as_ref(), "Alice");
    assert_eq!(m.mask_value("status", "ok").as_ref(), "ok");
    assert_eq!(m.mask_value("count", "42").as_ref(), "42");
}

// ── Case-insensitivity ──────────────────────────────────────────

#[test]
fn field_name_masking_is_case_insensitive() {
    let m = default_masker();
    assert_eq!(m.mask_value("Password", "x").as_ref(), "[REDACTED]");
    assert_eq!(m.mask_value("PASSWORD", "x").as_ref(), "[REDACTED]");
    assert_eq!(m.mask_value("pAsSwOrD", "x").as_ref(), "[REDACTED]");
    assert_eq!(m.mask_value("TOKEN", "x").as_ref(), "[REDACTED]");
    assert_eq!(m.mask_value("Api_Key", "x").as_ref(), "[REDACTED]");
}

// ── Value-pattern masking ───────────────────────────────────────

#[test]
fn masks_email_in_value() {
    let m = default_masker();
    let result = m.mask_value("msg", "contact user@example.com today");
    assert_eq!(result.as_ref(), "contact ***@***.*** today");
}

#[test]
fn masks_credit_card_with_dashes() {
    let m = default_masker();
    let result = m.mask_value("info", "card 4111-1111-1111-1234 used");
    assert_eq!(result.as_ref(), "card ****-****-****-1234 used");
}

#[test]
fn masks_credit_card_no_separators() {
    let m = default_masker();
    let result = m.mask_value("info", "card 4111111111111234 used");
    assert_eq!(result.as_ref(), "card ****-****-****-1234 used");
}

#[test]
fn masks_credit_card_with_spaces() {
    let m = default_masker();
    let result = m.mask_value("data", "4111 1111 1111 9999");
    assert_eq!(result.as_ref(), "****-****-****-9999");
}

#[test]
fn credit_card_preserves_last_four_digits() {
    let m = default_masker();
    let result = m.mask_value("data", "card 1234-5678-9012-3456 end");
    assert!(
        result.contains("3456"),
        "last four digits missing: {}",
        result
    );
    assert!(
        result.contains("****-****-****-"),
        "mask prefix missing: {}",
        result
    );
}

#[test]
fn masks_ssn_in_value() {
    let m = default_masker();
    let result = m.mask_value("data", "ssn is 123-45-6789");
    assert_eq!(result.as_ref(), "ssn is ***-**-****");
}

#[test]
fn masks_ssn_without_dashes() {
    let m = default_masker();
    let result = m.mask_value("data", "ssn is 123456789");
    assert_eq!(result.as_ref(), "ssn is ***-**-****");
}

#[test]
fn masks_jwt_in_value() {
    let m = default_masker();
    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abc123def456";
    let input = format!("token is {}", jwt);
    let result = m.mask_value("data", &input);
    assert_eq!(result.as_ref(), "token is [JWT_REDACTED]");
}

#[test]
fn masks_bearer_token_in_value() {
    let m = default_masker();
    let result = m.mask_value("header", "Bearer abc123def456.xyz");
    assert_eq!(result.as_ref(), "Bearer [REDACTED]");
}

#[test]
fn masks_aws_access_key_in_value() {
    let m = default_masker();
    let result = m.mask_value("data", "key is AKIAIOSFODNN7EXAMPLE");
    assert_eq!(result.as_ref(), "key is [AWS_KEY_REDACTED]");
}

#[test]
fn masks_hex_secret_in_value() {
    let m = default_masker();
    let hex = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4"; // 32 hex chars
    let input = format!("secret: {}", hex);
    let result = m.mask_value("data", &input);
    assert_eq!(result.as_ref(), "secret: [HEX_REDACTED]");
}

// ── Custom config ───────────────────────────────────────────────

#[test]
fn custom_field_names_are_masked() {
    let cfg = MaskingConfig {
        field_names: vec!["custom_secret".to_string()],
        ..Default::default()
    };
    let m = DefaultMasker::new(&cfg).expect("valid custom field config");
    assert_eq!(
        m.mask_value("custom_secret", "hidden").as_ref(),
        "[REDACTED]"
    );
}

#[test]
fn custom_replacement_string() {
    let cfg = MaskingConfig {
        replacement: "***".to_string(),
        ..Default::default()
    };
    let m = DefaultMasker::new(&cfg).expect("valid replacement config");
    assert_eq!(m.mask_value("password", "secret").as_ref(), "***");
}

#[test]
fn custom_value_patterns_applied() {
    let cfg = MaskingConfig {
        value_patterns: vec![r"secret_\w+".to_string()],
        ..Default::default()
    };
    let m = DefaultMasker::new(&cfg).expect("valid custom value pattern");
    let result = m.mask_value("msg", "found secret_abc123 in log");
    assert_eq!(result.as_ref(), "found [REDACTED] in log");
}

#[test]
fn invalid_custom_pattern_returns_error() {
    let cfg = MaskingConfig {
        value_patterns: vec!["[invalid".to_string()],
        ..Default::default()
    };
    let err = match DefaultMasker::new(&cfg) {
        Ok(_) => panic!("invalid regex should fail setup"),
        Err(err) => err,
    };
    assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidFormat);
}

// ── Disabled masking ────────────────────────────────────────────

#[test]
fn disabled_masker_passes_through() {
    let cfg = MaskingConfig {
        enabled: false,
        ..Default::default()
    };
    let m = DefaultMasker::new(&cfg).expect("valid disabled config");
    assert_eq!(m.mask_value("password", "hunter2").as_ref(), "hunter2");
}

#[test]
fn disabled_mask_output_passes_through() {
    let cfg = MaskingConfig {
        enabled: false,
        ..Default::default()
    };
    let m = DefaultMasker::new(&cfg).expect("valid disabled config");
    let line = "{\"password\":\"secret\"}";
    assert_eq!(m.mask_output(line).as_ref(), line);
}

// ── Default config ──────────────────────────────────────────────

#[test]
fn default_config_is_enabled() {
    let cfg = MaskingConfig::default();
    assert!(cfg.enabled);
    assert_eq!(cfg.replacement, "[REDACTED]");
    assert!(cfg.field_names.is_empty());
    assert!(cfg.value_patterns.is_empty());
}

// ── mask_output (line-level) ────────────────────────────────────

#[test]
fn mask_output_masks_json_field_values() {
    let m = default_masker();
    let input = "{\"password\":\"secret123\",\"name\":\"Alice\"}";
    let output = m.mask_output(input);
    assert!(
        output.contains("\"password\":\"[REDACTED]\""),
        "output: {}",
        output
    );
    assert!(output.contains("\"name\":\"Alice\""), "output: {}", output);
}

#[test]
fn mask_output_masks_text_field_values() {
    let m = default_masker();
    let input = "password=secret123 name=Alice";
    let output = m.mask_output(input);
    assert!(output.contains("password=[REDACTED]"), "output: {}", output);
    assert!(output.contains("name=Alice"), "output: {}", output);
}

#[test]
fn mask_output_masks_email_in_line() {
    let m = default_masker();
    let output = m.mask_output("user email: user@example.com logged in");
    assert!(output.contains("***@***.***"), "output: {}", output);
}

#[test]
fn mask_output_returns_borrowed_when_nothing_to_mask() {
    let m = default_masker();
    let result = m.mask_output("nothing sensitive here");
    assert!(matches!(result, Cow::Borrowed(_)));
}

// ── Standalone function ─────────────────────────────────────────

#[test]
fn standalone_mask_value_works() {
    assert_eq!(mask_value("password", "hunter2"), "[REDACTED]");
    assert_eq!(mask_value("status", "ok"), "ok");
}

// ── Thread safety ───────────────────────────────────────────────

#[test]
fn masker_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DefaultMasker>();
}

#[test]
fn masker_works_across_threads() {
    let masker: Arc<DefaultMasker> = Arc::new(DefaultMasker::default());
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let m = Arc::clone(&masker);
            std::thread::spawn(move || {
                let input = format!("secret-{}", i);
                let result = m.mask_value("password", &input);
                assert_eq!(result.as_ref(), "[REDACTED]");
            })
        })
        .collect();
    for h in handles {
        h.join().expect("thread panicked");
    }
}

// ── Serde deserialization ───────────────────────────────────────

#[test]
fn config_deserializes_from_json() {
    let json = "{\"enabled\":true,\"field_names\":[\"custom\"],\"replacement\":\"***\"}";
    let cfg: MaskingConfig = serde_json::from_str(json).unwrap();
    assert!(cfg.enabled);
    assert_eq!(cfg.field_names, vec!["custom"]);
    assert_eq!(cfg.replacement, "***");
}

#[test]
fn config_deserializes_with_defaults() {
    let json = "{}";
    let cfg: MaskingConfig = serde_json::from_str(json).unwrap();
    assert!(cfg.enabled);
    assert!(cfg.field_names.is_empty());
    assert_eq!(cfg.replacement, "[REDACTED]");
}

// ── Zero-copy ───────────────────────────────────────────────────

#[test]
fn returns_borrowed_for_unmasked_value() {
    let m = default_masker();
    let result = m.mask_value("name", "Alice");
    assert!(matches!(result, Cow::Borrowed(_)));
}

#[test]
fn returns_owned_for_masked_value() {
    let m = default_masker();
    let result = m.mask_value("password", "secret");
    assert!(matches!(result, Cow::Owned(_)));
}
