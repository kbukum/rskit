#![no_main]
use libfuzzer_sys::fuzz_target;

// Untrusted input crosses a trust boundary when a peer's RFC 9457 error
// response is deserialized. Parsing arbitrary bytes must never panic, and a
// successfully parsed `ProblemDetail` must serialize and round-trip back
// through the deserializer without error.
fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    let _ = serde_json::from_str::<rskit_errors::ErrorCode>(text);

    if let Ok(pd) = serde_json::from_str::<rskit_errors::ProblemDetail>(text) {
        let json =
            serde_json::to_string(&pd).expect("serializing a parsed ProblemDetail must not fail");
        serde_json::from_str::<rskit_errors::ProblemDetail>(&json)
            .expect("serialized ProblemDetail must deserialize back");
    }
});
