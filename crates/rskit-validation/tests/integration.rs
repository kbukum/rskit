use rskit_validation::{validate_email, validate_url, validate_uuid, Validator};

// ── required ──────────────────────────────────────────────────────────────────

#[test]
fn required_passes_for_non_empty_value() {
    let result = Validator::new().required("name", "Alice").validate();
    assert!(result.is_ok());
}

#[test]
fn required_fails_for_empty_string() {
    let err = Validator::new().required("name", "").validate().unwrap_err();
    assert_eq!(err.code, rskit_errors::ErrorCode::InvalidInput);
    assert!(err.message.contains("name"));
}

#[test]
fn required_fails_for_whitespace_only() {
    let err = Validator::new().required("name", "   ").validate().unwrap_err();
    assert!(err.message.contains("name"));
}

// ── min_length / max_length ───────────────────────────────────────────────────

#[test]
fn min_length_passes_at_boundary() {
    let result = Validator::new().min_length("pw", "abc", 3).validate();
    assert!(result.is_ok());
}

#[test]
fn min_length_fails_below_boundary() {
    let err = Validator::new()
        .min_length("pw", "ab", 3)
        .validate()
        .unwrap_err();
    assert!(err.message.contains("pw"));
}

#[test]
fn max_length_passes_at_boundary() {
    let result = Validator::new().max_length("bio", "abc", 3).validate();
    assert!(result.is_ok());
}

#[test]
fn max_length_fails_above_boundary() {
    let err = Validator::new()
        .max_length("bio", "abcd", 3)
        .validate()
        .unwrap_err();
    assert!(err.message.contains("bio"));
}

// ── email ─────────────────────────────────────────────────────────────────────

#[test]
fn email_passes_for_valid_address() {
    let result = Validator::new().email("email", "user@example.com").validate();
    assert!(result.is_ok());
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
    let err = Validator::new()
        .email("email", "")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("email"));
}

// ── pattern ───────────────────────────────────────────────────────────────────

#[test]
fn pattern_passes_when_value_matches_regex() {
    let result = Validator::new()
        .pattern("zip", "90210", r"^\d{5}$")
        .validate();
    assert!(result.is_ok());
}

#[test]
fn pattern_fails_when_value_does_not_match() {
    let err = Validator::new()
        .pattern("zip", "9021", r"^\d{5}$")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("zip"));
}

// ── one_of ────────────────────────────────────────────────────────────────────

#[test]
fn one_of_passes_for_valid_value() {
    let result = Validator::new()
        .one_of("role", &"admin", &["admin", "user", "guest"])
        .validate();
    assert!(result.is_ok());
}

#[test]
fn one_of_fails_for_invalid_value() {
    let err = Validator::new()
        .one_of("role", &"superuser", &["admin", "user", "guest"])
        .validate()
        .unwrap_err();
    assert!(err.message.contains("role"));
}

// ── chained validations collect all errors ────────────────────────────────────

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
    // All three field names should appear in the joined message
    assert!(err.message.contains("name"));
    assert!(err.message.contains("email"));
    assert!(err.message.contains("pw"));
}

#[test]
fn validate_returns_ok_when_all_pass() {
    let result = Validator::new()
        .required("name", "Alice")
        .email("email", "alice@example.com")
        .min_length("pw", "longpassword", 8)
        .max_length("pw", "longpassword", 64)
        .validate();
    assert!(result.is_ok());
}

// ── free functions ────────────────────────────────────────────────────────────

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

// ── in_range ──────────────────────────────────────────────────────────────────

#[test]
fn in_range_passes_within_bounds() {
    let result = Validator::new().in_range("age", 25, 1, 120).validate();
    assert!(result.is_ok());
}

#[test]
fn in_range_fails_below_min() {
    let err = Validator::new()
        .in_range("age", 0, 1, 120)
        .validate()
        .unwrap_err();
    assert!(err.message.contains("age"));
}

#[test]
fn in_range_fails_above_max() {
    let err = Validator::new()
        .in_range("age", 200, 1, 120)
        .validate()
        .unwrap_err();
    assert!(err.message.contains("age"));
}

// ── custom ────────────────────────────────────────────────────────────────────

#[test]
fn custom_passes_when_check_is_true() {
    let result = Validator::new()
        .custom("tos", true, "must accept terms")
        .validate();
    assert!(result.is_ok());
}

#[test]
fn custom_fails_when_check_is_false() {
    let err = Validator::new()
        .custom("tos", false, "must accept terms")
        .validate()
        .unwrap_err();
    assert!(err.message.contains("tos"));
}
