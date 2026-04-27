#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Fuzz JWT decoding — should never panic, only return Err
        let _ = rskit_auth::jwt::JwtService::decode_unverified(s);
    }
});
