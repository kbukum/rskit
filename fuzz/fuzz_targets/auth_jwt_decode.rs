#![no_main]
use jsonwebtoken::decode_header;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Fuzz JWT header parsing — should never panic, only return Err.
        let _ = decode_header(s);
    }
});
