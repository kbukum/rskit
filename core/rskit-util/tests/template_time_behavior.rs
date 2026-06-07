//! Behavioral coverage for utility template errors and clock helpers.

use rskit_util::template::TemplateError;
use rskit_util::time::{Clock, FixedClock, SystemClock, system_clock};

#[test]
fn template_errors_have_actionable_display_messages() {
    let cases = [
        (
            TemplateError::UnclosedPlaceholder("hello {name".to_owned()),
            "unclosed placeholder in 'hello {name'",
        ),
        (
            TemplateError::UnmatchedClosingBrace("hello }".to_owned()),
            "unmatched closing placeholder brace in 'hello }'",
        ),
        (
            TemplateError::EmptyPlaceholder,
            "placeholder cannot be empty",
        ),
        (
            TemplateError::UnknownPlaceholder("name".to_owned()),
            "unknown placeholder 'name'",
        ),
        (
            TemplateError::Render("callback failed".to_owned()),
            "template render failed: callback failed",
        ),
    ];

    for (error, message) in cases {
        assert_eq!(error.to_string(), message);
        let source: &dyn std::error::Error = &error;
        assert!(source.source().is_none());
    }
}

#[test]
fn system_and_fixed_clocks_expose_monotonic_and_epoch_values() {
    let fixed = FixedClock::new(123, 456);
    assert_eq!(fixed.epoch_seconds(), 123);
    assert_eq!(fixed.monotonic_millis(), 456);

    let system = SystemClock::new();
    assert!(system.epoch_seconds() > 0);
    assert!(system.monotonic_millis() < 1_000);

    let shared = system_clock();
    assert!(shared.epoch_seconds() > 0);
}
