use crate::aes_gcm::AesGcmEncryptor;
use crate::chacha20::ChaCha20Encryptor;
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
