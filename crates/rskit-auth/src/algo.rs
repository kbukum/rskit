//! JWT algorithm typestate — prevents algorithm confusion attacks at compile time.

mod sealed {
    pub trait Sealed {}
}

/// Marker trait for a JWT signing/verification algorithm.
pub trait JwtAlgo: sealed::Sealed + Send + Sync + 'static {
    const ALG: jsonwebtoken::Algorithm;
}

/// HMAC-SHA256 symmetric algorithm.
pub struct Hs256;
/// RSA-SHA256 asymmetric algorithm.
pub struct Rs256;
/// ECDSA-SHA256 asymmetric algorithm.
pub struct Es256;

impl sealed::Sealed for Hs256 {}
impl sealed::Sealed for Rs256 {}
impl sealed::Sealed for Es256 {}

impl JwtAlgo for Hs256 {
    const ALG: jsonwebtoken::Algorithm = jsonwebtoken::Algorithm::HS256;
}
impl JwtAlgo for Rs256 {
    const ALG: jsonwebtoken::Algorithm = jsonwebtoken::Algorithm::RS256;
}
impl JwtAlgo for Es256 {
    const ALG: jsonwebtoken::Algorithm = jsonwebtoken::Algorithm::ES256;
}
