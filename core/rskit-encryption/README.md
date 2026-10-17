# rskit-encryption

Symmetric encryption utilities with support for AES-256-GCM and ChaCha20-Poly1305.

## Features

- **AES-256-GCM**: Default algorithm with hardware acceleration on modern CPUs
- **ChaCha20-Poly1305**: Modern AEAD cipher, fast on CPUs without AES-NI
- **PBKDF2-SHA256**: Passphrase stretching with 600,000 iterations and random salt
- **HKDF-SHA256**: Cheap derivation for random key material such as mounted key files
- **Versioned envelope**: Ciphertexts carry format and algorithm identifiers
- **Thread-safe**: All encryptors are `Send + Sync` for use across async boundaries
- **Automatic nonce handling**: Random nonce generated for each encryption
- **Base64 encoding**: Ciphertext is automatically base64-encoded for safe transmission
- **Zeroize**: Key material is zeroized on drop

## Usage

```rust
use rskit_encryption::{new_encryptor, Algorithm};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an AES-GCM encryptor (default)
    let encryptor = new_encryptor(b"my-secret-key", Algorithm::AesGcm);

    let plaintext = b"sensitive data";
    let ciphertext = encryptor.encrypt(plaintext)?;
    let decrypted = encryptor.decrypt(&ciphertext)?;

    assert_eq!(decrypted, plaintext);
    Ok(())
}
```

## Algorithms

### AES-256-GCM
- **Default choice** for most applications
- Hardware acceleration available on modern x86-64 and ARM CPUs
- NIST standard

### ChaCha20-Poly1305
- **Best for** systems without AES hardware acceleration
- Modern construction from DJB
- Used in TLS 1.3

## Key Derivation

Every encryption derives a fresh message key from the master key and a random 16-byte salt stored in the envelope. How it derives depends on what the master key is:

- **Passphrase** (`new_encryptor`, `::new`): a human-chosen secret is stretched with PBKDF2-SHA256 at 600,000 iterations. This costs tens of milliseconds per operation, by design.
- **Key material** (`new_key_material_encryptor`, `::from_key_material`): a uniformly random key of at least 32 bytes, such as a mounted key file, is expanded with HKDF-SHA256 using the info string `kit.encryption.v2/<algorithm name>`. This takes microseconds. Shorter keys are refused with `InvalidInput`.

```rust
use rskit_encryption::{new_key_material_encryptor, Algorithm};

fn seal(mounted_key: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    let encryptor = new_key_material_encryptor(mounted_key, Algorithm::AesGcm)?;
    Ok(encryptor.encrypt(b"sensitive data")?)
}
```

## Ciphertext Format

```
base64(version[1] || algorithm[1] || salt[16] || nonce[12] || ciphertext)
```

The version records the derivation: 1 for a passphrase, 2 for key material. An encryptor opens only its own version, so a passphrase never opens key-material ciphertext or the reverse. The header is authenticated as AEAD associated data, so tampering with it fails decryption. gokit's `encryption` package produces the same envelopes; both kits test the same vectors.

## Error Handling

All operations return `AppResult<T>` for consistent error handling across rskit:

- Encryption errors are reported as `Internal`
- Decryption errors (wrong key, corrupted data) are reported as `InvalidFormat`
- Base64 decode errors are reported as `InvalidFormat`

## Security Considerations

- Each encryption generates a unique random salt and nonce
- PBKDF2 with 600k iterations resists brute-force and rainbow table attacks on passphrases
- Use key material only for random keys; a human-chosen password needs the passphrase mode
- Both algorithms provide authenticated encryption (AEAD)
- Key material is zeroized when the encryptor is dropped
- The same plaintext encrypted twice produces different ciphertext
