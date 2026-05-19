#![no_main]

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use libfuzzer_sys::fuzz_target;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    iss: String,
    aud: Vec<String>,
    exp: u64,
    nbf: Option<u64>,
    iat: Option<u64>,
}

fuzz_target!(|data: &[u8]| {
    if let Ok(token) = std::str::from_utf8(data) {
        let _ = decode_header(token);

        let mut validation = Validation::new(Algorithm::HS256);
        validation.algorithms = vec![Algorithm::HS256];
        validation.set_issuer(&["https://issuer.rskit.test"]);
        validation.set_audience(&["rskit-fuzz"]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);

        if let Ok(data) = decode::<Claims>(
            token,
            &DecodingKey::from_secret(b"fuzz-secret-key-32-bytes-minimum"),
            &validation,
        ) {
            let _ = (
                data.claims.sub,
                data.claims.iss,
                data.claims.aud,
                data.claims.exp,
                data.claims.nbf,
                data.claims.iat,
            );
        }
    }
});
