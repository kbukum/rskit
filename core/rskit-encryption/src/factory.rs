use crate::aes_gcm::AesGcmEncryptor;
use crate::chacha20::ChaCha20Encryptor;
use rskit_errors::AppResult;

use crate::traits::{Algorithm, Encryptor};

/// Creates a new encryptor for the specified algorithm.
///
/// # Arguments
///
/// * `key` - The encryption passphrase (used with PBKDF2-SHA256 to derive keys)
/// * `algorithm` - The encryption algorithm to use
///
/// # Returns
///
/// A boxed trait object implementing [`Encryptor`].
///
/// # Examples
///
/// ```no_run
/// use rskit_encryption::{new_encryptor, Algorithm};
///
/// let encryptor = new_encryptor(b"secret-key", Algorithm::ChaCha20Poly1305);
/// ```
pub fn new_encryptor(key: &[u8], algorithm: Algorithm) -> Box<dyn Encryptor> {
    match algorithm {
        Algorithm::AesGcm => Box::new(AesGcmEncryptor::new(key)),
        Algorithm::ChaCha20Poly1305 => Box::new(ChaCha20Encryptor::new(key)),
    }
}

/// Creates an encryptor that treats `key` as uniformly random key material of at least 32 bytes,
/// such as a mounted key file, and derives message keys with HKDF-SHA256.
///
/// Its envelopes use version 2 and never open passphrase (version 1) ciphertext, or the reverse.
///
/// # Errors
///
/// Returns `InvalidInput` when the key is shorter than 32 bytes.
pub fn new_key_material_encryptor(
    key: &[u8],
    algorithm: Algorithm,
) -> AppResult<Box<dyn Encryptor>> {
    Ok(match algorithm {
        Algorithm::AesGcm => Box::new(AesGcmEncryptor::from_key_material(key)?),
        Algorithm::ChaCha20Poly1305 => Box::new(ChaCha20Encryptor::from_key_material(key)?),
    })
}
