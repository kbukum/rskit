use super::*;

#[test]
fn test_factory_aes_gcm() {
    let encryptor = new_encryptor(b"secret-key", Algorithm::AesGcm);
    assert_eq!(encryptor.algorithm(), Algorithm::AesGcm);

    let plaintext = b"test";
    let ciphertext = encryptor.encrypt(plaintext).unwrap();
    let decrypted = encryptor.decrypt(&ciphertext).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_factory_chacha20() {
    let encryptor = new_encryptor(b"secret-key", Algorithm::ChaCha20Poly1305);
    assert_eq!(encryptor.algorithm(), Algorithm::ChaCha20Poly1305);

    let plaintext = b"test";
    let ciphertext = encryptor.encrypt(plaintext).unwrap();
    let decrypted = encryptor.decrypt(&ciphertext).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn malformed_versioned_envelopes_are_rejected() {
    use base64::{Engine, engine::general_purpose::STANDARD};

    let encryptor = new_encryptor(b"secret-key", Algorithm::AesGcm);
    let too_short = STANDARD.encode([1_u8, 1, 2, 3]);
    let err = encryptor.decrypt(&too_short).unwrap_err();
    assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidFormat);

    let mut bad_version = vec![0_u8, 1];
    bad_version.extend_from_slice(&[0_u8; 44]);
    let err = encryptor
        .decrypt(&STANDARD.encode(bad_version))
        .unwrap_err();
    assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidFormat);

    let mut bad_algorithm = vec![1_u8, 99];
    bad_algorithm.extend_from_slice(&[0_u8; 44]);
    let err = encryptor
        .decrypt(&STANDARD.encode(bad_algorithm))
        .unwrap_err();
    assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidFormat);
}
