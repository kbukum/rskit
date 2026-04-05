use rskit_validation::{FieldError, Validator, validate_email, validate_url, validate_uuid};
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════════════════════
// ── required ──────────────────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn required_passes_for_non_empty_value() {
    let result = Validator::new().required("name", "Alice").validate();
    assert!(result.is_ok());
}

#[test]
fn required_fails_for_empty_string() {
    let err = Validator::new()
        .required("name", "")
        .validate()
        .unwrap_err();
    assert_eq!(err.code, rskit_errors::ErrorCode::InvalidInput);
    assert!(err.message.contains("name"));
}

#[test]
fn required_fails_for_whitespace_only() {
    let err = Validator::new()
        .required("name", "   ")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("name"));
}

#[test]
fn required_fails_for_tab_and_newline() {
    let err = Validator::new()
        .required("name", "\t\n\r")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("is required"));
}

#[test]
fn required_passes_for_single_char() {
    assert!(Validator::new().required("x", "a").validate().is_ok());
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── min_length / max_length ───────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn min_length_passes_at_boundary() {
    assert!(Validator::new().min_length("pw", "abc", 3).validate().is_ok());
}

#[test]
fn min_length_fails_below_boundary() {
    let err = Validator::new()
        .min_length("pw", "ab", 3)
        .validate()
        .unwrap_err();
    assert!(err.message.contains("pw"));
    assert!(err.message.contains("at least 3"));
}

#[test]
fn min_length_passes_above_boundary() {
    assert!(Validator::new().min_length("pw", "abcd", 3).validate().is_ok());
}

#[test]
fn min_length_fails_on_empty_string() {
    assert!(Validator::new().min_length("pw", "", 1).validate().is_err());
}

#[test]
fn min_length_passes_with_zero_min() {
    assert!(Validator::new().min_length("pw", "", 0).validate().is_ok());
}

#[test]
fn max_length_passes_at_boundary() {
    assert!(Validator::new().max_length("bio", "abc", 3).validate().is_ok());
}

#[test]
fn max_length_fails_above_boundary() {
    let err = Validator::new()
        .max_length("bio", "abcd", 3)
        .validate()
        .unwrap_err();
    assert!(err.message.contains("bio"));
    assert!(err.message.contains("at most 3"));
}

#[test]
fn max_length_passes_below_boundary() {
    assert!(Validator::new().max_length("bio", "ab", 3).validate().is_ok());
}

#[test]
fn max_length_passes_on_empty_string() {
    assert!(Validator::new().max_length("bio", "", 10).validate().is_ok());
}

#[test]
fn min_length_counts_unicode_chars_not_bytes() {
    // "héllo" is 5 chars but 6 bytes (é = 2 bytes)
    assert!(Validator::new().min_length("f", "héllo", 5).validate().is_ok());
    assert!(Validator::new().min_length("f", "héllo", 6).validate().is_err());
}

#[test]
fn max_length_counts_unicode_chars_not_bytes() {
    // 4 emoji chars (each 4 bytes = 16 bytes total, but 4 chars)
    let emoji = "🎉🎊🎈🎁";
    assert!(Validator::new().max_length("f", emoji, 4).validate().is_ok());
    assert!(Validator::new().max_length("f", emoji, 3).validate().is_err());
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── email ─────────────────────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn email_passes_for_valid_address() {
    assert!(Validator::new().email("email", "user@example.com").validate().is_ok());
}

#[test]
fn email_fails_for_missing_at() {
    let err = Validator::new()
        .email("email", "userexample.com")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("email"));
}

#[test]
fn email_fails_for_empty_string() {
    assert!(Validator::new().email("email", "").validate().is_err());
}

#[test]
fn email_fails_for_at_only() {
    assert!(!validate_email("@"));
}

#[test]
fn email_fails_for_no_local_part() {
    assert!(!validate_email("@example.com"));
}

#[test]
fn email_fails_for_domain_starting_with_dot() {
    assert!(!validate_email("user@.example.com"));
}

#[test]
fn email_fails_for_domain_ending_with_dot() {
    assert!(!validate_email("user@example.com."));
}

#[test]
fn email_fails_for_domain_without_dot() {
    assert!(!validate_email("user@localhost"));
}

#[test]
fn email_passes_for_subdomain() {
    assert!(validate_email("user@mail.example.com"));
}

#[test]
fn email_passes_for_plus_addressing() {
    assert!(validate_email("user+tag@example.com"));
}

#[test]
fn email_passes_for_dots_in_local() {
    assert!(validate_email("first.last@example.com"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── url ───────────────────────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn url_passes_for_https() {
    assert!(Validator::new().url("site", "https://example.com").validate().is_ok());
}

#[test]
fn url_passes_for_http() {
    assert!(Validator::new().url("site", "http://example.com").validate().is_ok());
}

#[test]
fn url_fails_for_ftp() {
    assert!(Validator::new().url("site", "ftp://example.com").validate().is_err());
}

#[test]
fn url_fails_for_empty() {
    assert!(Validator::new().url("site", "").validate().is_err());
}

#[test]
fn url_fails_for_plain_text() {
    assert!(Validator::new().url("site", "not a url").validate().is_err());
}

#[test]
fn url_passes_for_url_with_path_and_query() {
    assert!(validate_url("https://example.com/path?q=1&b=2#frag"));
}

#[test]
fn url_fails_for_javascript_protocol() {
    assert!(!validate_url("javascript:alert(1)"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── pattern ───────────────────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn pattern_passes_when_value_matches_regex() {
    assert!(Validator::new().pattern("zip", "90210", r"^\d{5}$").validate().is_ok());
}

#[test]
fn pattern_fails_when_value_does_not_match() {
    let err = Validator::new()
        .pattern("zip", "9021", r"^\d{5}$")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("zip"));
    assert!(err.message.contains("must match pattern"));
}

#[test]
fn pattern_fails_with_invalid_regex() {
    let err = Validator::new()
        .pattern("f", "value", "[invalid")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("invalid pattern"));
}

#[test]
fn pattern_passes_on_empty_string_matching_empty_pattern() {
    assert!(Validator::new().pattern("f", "", r"^$").validate().is_ok());
}

#[test]
fn pattern_matches_unicode() {
    assert!(Validator::new().pattern("f", "café", r"café").validate().is_ok());
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── one_of ────────────────────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn one_of_passes_for_valid_value() {
    assert!(
        Validator::new()
            .one_of("role", &"admin", &["admin", "user", "guest"])
            .validate()
            .is_ok()
    );
}

#[test]
fn one_of_fails_for_invalid_value() {
    let err = Validator::new()
        .one_of("role", &"superuser", &["admin", "user", "guest"])
        .validate()
        .unwrap_err();
    assert!(err.message.contains("role"));
    assert!(err.message.contains("must be one of"));
}

#[test]
fn one_of_fails_for_empty_allowed_list() {
    let empty: &[&str] = &[];
    assert!(
        Validator::new()
            .one_of("role", &"admin", empty)
            .validate()
            .is_err()
    );
}

#[test]
fn one_of_works_with_integers() {
    assert!(Validator::new().one_of("level", &3, &[1, 2, 3]).validate().is_ok());
    assert!(Validator::new().one_of("level", &4, &[1, 2, 3]).validate().is_err());
}

#[test]
fn one_of_error_message_lists_allowed_values() {
    let err = Validator::new()
        .one_of("color", &"pink", &["red", "green", "blue"])
        .validate()
        .unwrap_err();
    assert!(err.message.contains("red"));
    assert!(err.message.contains("green"));
    assert!(err.message.contains("blue"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── in_range ──────────────────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn in_range_passes_within_bounds() {
    assert!(Validator::new().in_range("age", 25, 1, 120).validate().is_ok());
}

#[test]
fn in_range_passes_at_min_boundary() {
    assert!(Validator::new().in_range("age", 1, 1, 120).validate().is_ok());
}

#[test]
fn in_range_passes_at_max_boundary() {
    assert!(Validator::new().in_range("age", 120, 1, 120).validate().is_ok());
}

#[test]
fn in_range_fails_below_min() {
    let err = Validator::new()
        .in_range("age", 0, 1, 120)
        .validate()
        .unwrap_err();
    assert!(err.message.contains("age"));
    assert!(err.message.contains("between 1 and 120"));
}

#[test]
fn in_range_fails_above_max() {
    assert!(Validator::new().in_range("age", 200, 1, 120).validate().is_err());
}

#[test]
fn in_range_works_with_floats() {
    assert!(Validator::new().in_range("score", 0.5_f64, 0.0, 1.0).validate().is_ok());
    assert!(Validator::new().in_range("score", 1.1_f64, 0.0, 1.0).validate().is_err());
    assert!(Validator::new().in_range("score", -0.1_f64, 0.0, 1.0).validate().is_err());
}

#[test]
fn in_range_works_with_negative_values() {
    assert!(Validator::new().in_range("temp", -10, -20, 50).validate().is_ok());
    assert!(Validator::new().in_range("temp", -21, -20, 50).validate().is_err());
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── UUID ──────────────────────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn required_uuid_passes_for_valid() {
    assert!(
        Validator::new()
            .required_uuid("id", "550e8400-e29b-41d4-a716-446655440000")
            .validate()
            .is_ok()
    );
}

#[test]
fn required_uuid_fails_for_invalid() {
    let err = Validator::new()
        .required_uuid("id", "not-a-uuid")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("id"));
    assert!(err.message.contains("valid UUID"));
}

#[test]
fn required_uuid_fails_for_empty_string() {
    assert!(Validator::new().required_uuid("id", "").validate().is_err());
}

#[test]
fn optional_uuid_passes_for_none() {
    assert!(Validator::new().optional_uuid("id", None).validate().is_ok());
}

#[test]
fn optional_uuid_passes_for_valid_some() {
    assert!(
        Validator::new()
            .optional_uuid("id", Some("550e8400-e29b-41d4-a716-446655440000"))
            .validate()
            .is_ok()
    );
}

#[test]
fn optional_uuid_fails_for_invalid_some() {
    let err = Validator::new()
        .optional_uuid("id", Some("garbage"))
        .validate()
        .unwrap_err();
    assert!(err.message.contains("id"));
}

#[test]
fn validate_uuid_accepts_uppercase() {
    assert!(validate_uuid("550E8400-E29B-41D4-A716-446655440000"));
}

#[test]
fn validate_uuid_rejects_too_short() {
    assert!(!validate_uuid("550e8400-e29b-41d4-a716"));
}

#[test]
fn validate_uuid_rejects_empty() {
    assert!(!validate_uuid(""));
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── before / after (datetime) ─────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn before_passes_when_value_is_before_deadline() {
    assert!(
        Validator::new()
            .before("start", "2024-01-01T00:00:00Z", "2025-01-01T00:00:00Z")
            .validate()
            .is_ok()
    );
}

#[test]
fn before_fails_when_value_equals_deadline() {
    assert!(
        Validator::new()
            .before("start", "2025-01-01T00:00:00Z", "2025-01-01T00:00:00Z")
            .validate()
            .is_err()
    );
}

#[test]
fn before_fails_when_value_is_after_deadline() {
    let err = Validator::new()
        .before("start", "2026-01-01T00:00:00Z", "2025-01-01T00:00:00Z")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("start"));
    assert!(err.message.contains("must be before"));
}

#[test]
fn before_fails_for_invalid_datetime() {
    let err = Validator::new()
        .before("start", "not-a-date", "2025-01-01T00:00:00Z")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("valid datetime"));
}

#[test]
fn after_passes_when_value_is_after_floor() {
    assert!(
        Validator::new()
            .after("end", "2026-01-01T00:00:00Z", "2025-01-01T00:00:00Z")
            .validate()
            .is_ok()
    );
}

#[test]
fn after_fails_when_value_equals_floor() {
    assert!(
        Validator::new()
            .after("end", "2025-01-01T00:00:00Z", "2025-01-01T00:00:00Z")
            .validate()
            .is_err()
    );
}

#[test]
fn after_fails_when_value_is_before_floor() {
    let err = Validator::new()
        .after("end", "2024-01-01T00:00:00Z", "2025-01-01T00:00:00Z")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("end"));
    assert!(err.message.contains("must be after"));
}

#[test]
fn after_fails_for_invalid_datetime() {
    let err = Validator::new()
        .after("end", "garbage", "2025-01-01T00:00:00Z")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("valid datetime"));
}

#[test]
fn before_skips_when_deadline_is_invalid() {
    // Invalid deadline but valid value => no error (falls through to _ match arm)
    assert!(
        Validator::new()
            .before("start", "2024-01-01T00:00:00Z", "bad-deadline")
            .validate()
            .is_ok()
    );
}

#[test]
fn after_skips_when_floor_is_invalid() {
    assert!(
        Validator::new()
            .after("end", "2026-01-01T00:00:00Z", "bad-floor")
            .validate()
            .is_ok()
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── custom ────────────────────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn custom_passes_when_check_is_true() {
    assert!(Validator::new().custom("tos", true, "must accept terms").validate().is_ok());
}

#[test]
fn custom_fails_when_check_is_false() {
    let err = Validator::new()
        .custom("tos", false, "must accept terms")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("tos"));
    assert!(err.message.contains("must accept terms"));
}

#[test]
fn custom_enables_arbitrary_domain_logic() {
    let password = "short";
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let err = Validator::new()
        .custom("password", has_digit, "must contain a digit")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("must contain a digit"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Validator builder / struct pattern ─────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn validator_default_has_no_errors() {
    let v = Validator::default();
    assert!(!v.has_errors());
    assert_eq!(v.errors().len(), 0);
}

#[test]
fn validator_new_returns_empty() {
    let v = Validator::new();
    assert!(v.errors().is_empty());
    assert!(v.validate().is_ok());
}

#[test]
fn validator_chaining_is_fluent() {
    let result = Validator::new()
        .required("a", "ok")
        .min_length("b", "hello", 1)
        .max_length("c", "hi", 10)
        .email("d", "x@y.com")
        .url("e", "https://x.com")
        .pattern("f", "123", r"\d+")
        .in_range("g", 5, 0, 10)
        .one_of("h", &1, &[1, 2, 3])
        .required_uuid("i", "550e8400-e29b-41d4-a716-446655440000")
        .optional_uuid("j", None)
        .custom("k", true, "ok")
        .before("l", "2024-01-01T00:00:00Z", "2025-01-01T00:00:00Z")
        .after("m", "2026-01-01T00:00:00Z", "2025-01-01T00:00:00Z")
        .validate();
    assert!(result.is_ok());
}

#[test]
fn has_errors_returns_false_when_all_pass() {
    let v = Validator::new().required("name", "Alice");
    assert!(!v.has_errors());
}

#[test]
fn has_errors_returns_true_when_any_fail() {
    let v = Validator::new().required("name", "");
    assert!(v.has_errors());
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── FieldError type properties ────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn field_error_has_field_and_message() {
    let fe = FieldError {
        field: "email".to_string(),
        message: "is required".to_string(),
    };
    assert_eq!(fe.field, "email");
    assert_eq!(fe.message, "is required");
}

#[test]
fn field_error_is_debug() {
    let fe = FieldError {
        field: "x".into(),
        message: "bad".into(),
    };
    let dbg = format!("{:?}", fe);
    assert!(dbg.contains("FieldError"));
    assert!(dbg.contains("x"));
}

#[test]
fn field_error_is_clone() {
    let fe = FieldError {
        field: "f".into(),
        message: "m".into(),
    };
    let cloned = fe.clone();
    assert_eq!(cloned.field, fe.field);
    assert_eq!(cloned.message, fe.message);
}

#[test]
fn errors_slice_matches_insertion_order() {
    let v = Validator::new()
        .required("first", "")
        .email("second", "bad")
        .min_length("third", "", 5);
    let errs = v.errors();
    assert_eq!(errs.len(), 3);
    assert_eq!(errs[0].field, "first");
    assert_eq!(errs[1].field, "second");
    assert_eq!(errs[2].field, "third");
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Multiple errors collected ─────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn validate_collects_multiple_field_errors() {
    let v = Validator::new()
        .required("name", "")
        .email("email", "bad")
        .min_length("pw", "ab", 8);

    assert!(v.has_errors());
    assert_eq!(v.errors().len(), 3);

    let err = v.validate().unwrap_err();
    assert_eq!(err.code, rskit_errors::ErrorCode::InvalidInput);
    assert!(err.message.contains("name"));
    assert!(err.message.contains("email"));
    assert!(err.message.contains("pw"));
}

#[test]
fn validate_error_message_joins_with_semicolons() {
    let err = Validator::new()
        .required("a", "")
        .required("b", "")
        .validate()
        .unwrap_err();
    // The message format is "field: msg; field: msg"
    assert!(err.message.contains("; "));
}

#[test]
fn validate_returns_ok_when_all_pass() {
    assert!(
        Validator::new()
            .required("name", "Alice")
            .email("email", "alice@example.com")
            .min_length("pw", "longpassword", 8)
            .max_length("pw", "longpassword", 64)
            .validate()
            .is_ok()
    );
}

#[test]
fn same_field_can_have_multiple_errors() {
    let v = Validator::new()
        .min_length("pw", "ab", 8)
        .pattern("pw", "ab", r".*\d.*");
    assert_eq!(v.errors().len(), 2);
    assert_eq!(v.errors()[0].field, "pw");
    assert_eq!(v.errors()[1].field, "pw");
}

#[test]
fn many_errors_all_appear_in_message() {
    let mut v = Validator::new();
    for i in 0..10 {
        let field = format!("field_{i}");
        v = v.required(&field, "");
    }
    let err = v.validate().unwrap_err();
    for i in 0..10 {
        assert!(err.message.contains(&format!("field_{i}")));
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Free functions ────────────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn validate_email_returns_true_for_valid() {
    assert!(validate_email("hello@world.com"));
}

#[test]
fn validate_email_returns_false_for_invalid() {
    assert!(!validate_email("not-an-email"));
}

#[test]
fn validate_url_returns_true_for_valid() {
    assert!(validate_url("https://example.com/path?q=1"));
}

#[test]
fn validate_url_returns_false_for_invalid() {
    assert!(!validate_url("not a url"));
}

#[test]
fn validate_uuid_returns_true_for_valid() {
    assert!(validate_uuid("550e8400-e29b-41d4-a716-446655440000"));
}

#[test]
fn validate_uuid_returns_false_for_invalid() {
    assert!(!validate_uuid("not-a-uuid"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Security: injection via error messages ────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn html_injection_in_field_name_is_preserved_literally() {
    let err = Validator::new()
        .required("<script>alert(1)</script>", "")
        .validate()
        .unwrap_err();
    // The field name must pass through literally, not be interpreted
    assert!(err.message.contains("<script>alert(1)</script>"));
}

#[test]
fn sql_injection_in_field_name_is_preserved_literally() {
    let err = Validator::new()
        .required("'; DROP TABLE users; --", "")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("'; DROP TABLE users; --"));
}

#[test]
fn custom_message_with_injection_payloads() {
    let err = Validator::new()
        .custom("f", false, "<img onerror=alert(1) src=x>")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("<img onerror=alert(1) src=x>"));
}

#[test]
fn null_bytes_in_field_name() {
    let err = Validator::new()
        .required("field\0name", "")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("field\0name"));
}

#[test]
fn extremely_long_field_name_does_not_panic() {
    let long_name = "x".repeat(100_000);
    let err = Validator::new()
        .required(&long_name, "")
        .validate()
        .unwrap_err();
    assert!(err.message.contains(&long_name));
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Security: ReDoS in pattern ────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn redos_pattern_does_not_hang() {
    // Classic ReDoS pattern: (a+)+ against "aaa...!" input
    // The Rust `regex` crate uses a finite-automaton approach, so it should
    // complete in bounded time. We verify it finishes within 2 seconds.
    let evil_input = "a".repeat(30) + "!";
    let start = Instant::now();
    let _result = Validator::new()
        .pattern("f", &evil_input, r"^(a+)+$")
        .validate();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "pattern validation took too long: {elapsed:?} — possible ReDoS"
    );
}

#[test]
fn nested_quantifier_redos_is_safe() {
    // Another classic ReDoS: nested quantifiers with exponential backtracking
    let evil_input = "a".repeat(50) + "X";
    let start = Instant::now();
    let _result = Validator::new()
        .pattern("f", &evil_input, r"^(a*)*b$")
        .validate();
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "nested quantifier regex took too long"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Edge cases: empty strings ─────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn empty_string_passes_email_check_only_with_validator() {
    // validate_email with empty string returns false
    assert!(!validate_email(""));
    assert!(Validator::new().email("e", "").validate().is_err());
}

#[test]
fn empty_string_passes_url_check() {
    assert!(!validate_url(""));
    assert!(Validator::new().url("u", "").validate().is_err());
}

#[test]
fn empty_string_fails_required_uuid() {
    assert!(!validate_uuid(""));
}

#[test]
fn empty_string_min_length_zero_passes() {
    assert!(Validator::new().min_length("f", "", 0).validate().is_ok());
}

#[test]
fn empty_string_max_length_zero_passes() {
    assert!(Validator::new().max_length("f", "", 0).validate().is_ok());
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Edge cases: unicode ───────────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn required_passes_for_unicode_content() {
    assert!(Validator::new().required("name", "日本語").validate().is_ok());
}

#[test]
fn required_passes_for_emoji() {
    assert!(Validator::new().required("name", "🚀").validate().is_ok());
}

#[test]
fn email_with_unicode_domain_fails_validation() {
    // IDN domains without punycode encoding: domain part has no dot
    // user@日本語 should fail (no dot in domain)
    assert!(!validate_email("user@日本語"));
}

#[test]
fn unicode_field_names_work() {
    let err = Validator::new()
        .required("名前", "")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("名前"));
}

#[test]
fn zero_width_space_is_not_empty_for_required() {
    // Zero-width space (U+200B) is not whitespace per Unicode,
    // so trim() won't remove it => it should pass required
    assert!(Validator::new().required("f", "\u{200B}").validate().is_ok());
}

#[test]
fn combining_characters_count_as_individual_chars() {
    // "é" as e + combining acute = 2 chars
    let decomposed = "e\u{0301}"; // 2 chars
    assert!(Validator::new().min_length("f", decomposed, 2).validate().is_ok());
    assert!(Validator::new().max_length("f", decomposed, 1).validate().is_err());
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Edge cases: very large inputs ─────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn very_long_string_passes_required() {
    let long = "a".repeat(1_000_000);
    assert!(Validator::new().required("f", &long).validate().is_ok());
}

#[test]
fn very_long_string_fails_max_length() {
    let long = "a".repeat(1_000_000);
    assert!(Validator::new().max_length("f", &long, 999_999).validate().is_err());
}

#[test]
fn very_long_string_passes_min_length() {
    let long = "a".repeat(1_000_000);
    assert!(Validator::new().min_length("f", &long, 999_999).validate().is_ok());
}

#[test]
fn very_large_range_values() {
    assert!(Validator::new().in_range("x", i64::MAX, i64::MIN, i64::MAX).validate().is_ok());
    assert!(Validator::new().in_range("x", i64::MIN, i64::MIN, i64::MAX).validate().is_ok());
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── Edge cases: Option::None ──────────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn optional_uuid_none_always_passes() {
    assert!(Validator::new().optional_uuid("id", None).validate().is_ok());
}

#[test]
fn optional_uuid_some_empty_fails() {
    assert!(Validator::new().optional_uuid("id", Some("")).validate().is_err());
}

#[test]
fn optional_uuid_some_whitespace_fails() {
    assert!(Validator::new().optional_uuid("id", Some("  ")).validate().is_err());
}

// ═══════════════════════════════════════════════════════════════════════════════
// ── End-to-end: realistic use case ────────────────────────────────────────────
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn realistic_user_registration_validation_passes() {
    let result = Validator::new()
        .required("username", "alice")
        .min_length("username", "alice", 3)
        .max_length("username", "alice", 30)
        .required("email", "alice@example.com")
        .email("email", "alice@example.com")
        .required("password", "Str0ng!Pass")
        .min_length("password", "Str0ng!Pass", 8)
        .max_length("password", "Str0ng!Pass", 128)
        .custom(
            "password",
            "Str0ng!Pass".chars().any(|c| c.is_ascii_digit()),
            "must contain a digit",
        )
        .in_range("age", 25, 13, 150)
        .one_of("role", &"user", &["admin", "user", "guest"])
        .validate();
    assert!(result.is_ok());
}

#[test]
fn realistic_user_registration_validation_fails() {
    let err = Validator::new()
        .required("username", "")
        .email("email", "not-valid")
        .min_length("password", "short", 8)
        .in_range("age", 5, 13, 150)
        .validate()
        .unwrap_err();

    assert_eq!(err.code, rskit_errors::ErrorCode::InvalidInput);
    assert!(err.message.contains("username"));
    assert!(err.message.contains("email"));
    assert!(err.message.contains("password"));
    assert!(err.message.contains("age"));
}

#[test]
fn validate_error_has_correct_http_status() {
    let err = Validator::new()
        .required("name", "")
        .validate()
        .unwrap_err();
    assert_eq!(err.http_status.as_u16(), 422); // UNPROCESSABLE_ENTITY
}

#[test]
fn validate_error_is_not_retryable() {
    let err = Validator::new()
        .required("name", "")
        .validate()
        .unwrap_err();
    assert!(!err.retryable);
}
