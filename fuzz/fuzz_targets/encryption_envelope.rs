#![no_main]

use libfuzzer_sys::fuzz_target;
use rskit_encryption::{Algorithm, new_encryptor};

fuzz_target!(|data: &[u8]| {
    let Ok(ciphertext) = std::str::from_utf8(data) else {
        return;
    };

    let aes = new_encryptor(b"fuzz-key", Algorithm::AesGcm);
    let chacha = new_encryptor(b"fuzz-key", Algorithm::ChaCha20Poly1305);

    let _ = aes.decrypt(ciphertext);
    let _ = chacha.decrypt(ciphertext);
});
