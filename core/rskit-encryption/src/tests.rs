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

// Shared with gokit's encryption/keymaterial_test.go: key = 0x00..0x1f, salt = 0x20..0x2f,
// nonce = 0x30..0x3b, plaintext "kit key material", no caller binding.
const KAT_PLAINTEXT: &[u8] = b"kit key material";
const KAT_VECTORS: [(Algorithm, &str); 2] = [
    (
        Algorithm::AesGcm,
        "AgEgISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7D0XZ4BESJRUjDeZ3hOvn9ERBGZ6pjHLIBfihpTxn3rI=",
    ),
    (
        Algorithm::ChaCha20Poly1305,
        "AgIgISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7woDWa+sQYBAxRPgz15Yz9B00vxuHxjes+ii1NOSwKcI=",
    ),
];

fn kat_bytes<const N: usize>(start: u8) -> [u8; N] {
    std::array::from_fn(|i| start + i as u8)
}

#[test]
fn key_material_matches_shared_vectors() {
    let key: [u8; 32] = kat_bytes(0x00);
    for (algorithm, want) in KAT_VECTORS {
        let master = crate::key::MasterKey::key_material(&key).unwrap();
        let sealed = master
            .seal(algorithm, KAT_PLAINTEXT, &kat_bytes(0x20), &kat_bytes(0x30))
            .unwrap();
        assert_eq!(sealed, want, "{algorithm}");

        let encryptor = new_key_material_encryptor(&key, algorithm).unwrap();
        assert_eq!(
            encryptor.decrypt(want).unwrap(),
            KAT_PLAINTEXT,
            "{algorithm}"
        );
    }
}

#[test]
fn key_material_round_trips_in_version_two_envelopes() {
    use base64::{Engine, engine::general_purpose::STANDARD};

    for algorithm in [Algorithm::AesGcm, Algorithm::ChaCha20Poly1305] {
        let encryptor = new_key_material_encryptor(&[7_u8; 32], algorithm).unwrap();
        assert_eq!(encryptor.algorithm(), algorithm);
        let ciphertext = encryptor.encrypt(b"secret").unwrap();
        assert_eq!(STANDARD.decode(&ciphertext).unwrap()[0], 2);
        assert_eq!(encryptor.decrypt(&ciphertext).unwrap(), b"secret");

        let other = new_key_material_encryptor(&[8_u8; 32], algorithm).unwrap();
        assert!(other.decrypt(&ciphertext).is_err());
    }
}

#[test]
fn short_key_material_is_refused() {
    for algorithm in [Algorithm::AesGcm, Algorithm::ChaCha20Poly1305] {
        let err = new_key_material_encryptor(&[7_u8; 31], algorithm)
            .err()
            .unwrap();
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }
    assert!(AesGcmEncryptor::from_key_material(&[7_u8; 31]).is_err());
    assert!(ChaCha20Encryptor::from_key_material(&[7_u8; 31]).is_err());
}

#[test]
fn passphrase_and_key_material_never_open_each_other() {
    let key = [7_u8; 32];
    for algorithm in [Algorithm::AesGcm, Algorithm::ChaCha20Poly1305] {
        let passphrase = new_encryptor(&key, algorithm);
        let material = new_key_material_encryptor(&key, algorithm).unwrap();

        let err = passphrase
            .decrypt(&material.encrypt(b"x").unwrap())
            .unwrap_err();
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidFormat);
        let err = material
            .decrypt(&passphrase.encrypt(b"x").unwrap())
            .unwrap_err();
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidFormat);
    }
}
