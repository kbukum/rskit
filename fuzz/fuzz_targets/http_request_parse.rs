#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz HTTP header parsing — should never panic
    let _ = http::Request::builder()
        .uri(std::str::from_utf8(data).unwrap_or("/"))
        .body(());
});
