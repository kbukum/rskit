# rskit-encryption

Symmetric encryption utilities with support for AES-256-GCM and ChaCha20-Poly1305.

## Features

- **AES-256-GCM**: Default algorithm with hardware acceleration on modern CPUs
- **ChaCha20-Poly1305**: Modern AEAD cipher, fast on CPUs without AES-NI
- **Thread-safe**: All encryptors are `Send + Sync` for use across async boundaries
- **Automatic nonce handling**: Random nonce generated for each encryption
- **Base64 encoding**: Ciphertext is automatically base64-encoded for safe transmission
- **Key derivation**: Keys are automatically hashed to 32 bytes with SHA-256

## Usage

```rust
use rskit_encryption::{new_encryptor, Algorithm};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an AES-GCM encryptor (default)
    let encryptor = new_encryptor(b"my-secret-key", Algorithm::AesGcm)?;

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
- Nonce size: 12 bytes

### ChaCha20-Poly1305
- **Best for** systems without AES hardware acceleration
- Modern construction from DJB
- Used in TLS 1.3
- Nonce size: 12 bytes

## Implementation Details

- Each encryption operation generates a random 12-byte nonce
- The nonce is prepended to the ciphertext and included in the base64 output
- This means the same plaintext encrypted twice produces different ciphertext
- Key derivation uses SHA-256 to ensure consistent 32-byte keys regardless of input length
- All operations use standard authenticated encryption with associated data (AEAD)

## Error Handling

All operations return `AppResult<T>` for consistent error handling across rskit:

- Encryption errors are reported as `InternalServerError`
- Decryption errors (wrong key, corrupted data) are reported as `BadRequest`
- Base64 decode errors are reported as `BadRequest`

## Performance

Both algorithms provide excellent performance for most use cases:

- AES-GCM: ~3-5 GB/s with AES-NI
- ChaCha20-Poly1305: ~1-2 GB/s, more consistent across platforms

Choose based on your requirements:
- Use AES-GCM if you have hardware acceleration available
- Use ChaCha20-Poly1305 for better portable performance
