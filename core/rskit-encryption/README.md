# rskit-encryption

Symmetric encryption utilities with support for AES-256-GCM and ChaCha20-Poly1305.

## Features

- **AES-256-GCM**: Default algorithm with hardware acceleration on modern CPUs
- **ChaCha20-Poly1305**: Modern AEAD cipher, fast on CPUs without AES-NI
- **PBKDF2-SHA256**: Key derivation with 600,000 iterations and random salt
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

Keys are derived from passphrases using **PBKDF2-SHA256** with:
- **600,000 iterations**
- **Random 16-byte salt** per encryption operation
- Salt is prepended to the ciphertext for extraction during decryption

## Ciphertext Format

```
base64(version[1] || algorithm[1] || salt[16] || nonce[12] || ciphertext)
```

The version and algorithm header is authenticated as AEAD associated data, so
tampering with envelope metadata fails decryption.

## Error Handling

All operations return `AppResult<T>` for consistent error handling across rskit:

- Encryption errors are reported as `Internal`
- Decryption errors (wrong key, corrupted data) are reported as `InvalidFormat`
- Base64 decode errors are reported as `InvalidFormat`

## Security Considerations

- Each encryption generates a unique random salt and nonce
- PBKDF2 with 600k iterations resists brute-force and rainbow table attacks
- Both algorithms provide authenticated encryption (AEAD)
- Key material is zeroized when the encryptor is dropped
- The same plaintext encrypted twice produces different ciphertext
