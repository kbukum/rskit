#![no_main]

use libfuzzer_sys::fuzz_target;
use rskit_validation::{Validator, input};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    let _ = input::reject_unicode_controls("field", text);
    let _ = input::validate_required_trimmed("field", text);
    let _ = input::validate_path_safe_identifier("field", text);
    let _ = input::validate_safe_path(text);
    let _ = input::validate_optional_trimmed("field", Some(text.to_owned()));

    let _ = Validator::new()
        .required("field", text)
        .max_length("field", text, 256)
        .email("email", text)
        .url("url", text)
        .required_uuid("id", text)
        .pattern("field", text, r"^[[:alnum:]_./:-]{0,256}$")
        .validate();
});
