//! Minimal Ed25519 signing/verification used by the barter protocol.

use ring::signature::{self, Ed25519KeyPair, KeyPair as _};
use vitalis_core::{Error, Result};

/// An Ed25519 signing identity.
pub struct Signer {
    inner: Ed25519KeyPair,
}

impl Signer {
    /// Generate a fresh signing identity.
    pub fn generate() -> Result<Self> {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
            .map_err(|e| Error::Integrity(format!("negotiate keygen: {e}")))?;
        let inner = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
            .map_err(|e| Error::Integrity(format!("negotiate key parse: {e}")))?;
        Ok(Self { inner })
    }

    /// The public key bytes (sent with each signed message).
    pub fn public_key(&self) -> Vec<u8> {
        self.inner.public_key().as_ref().to_vec()
    }

    /// Sign `data`, returning the raw signature.
    pub fn sign(&self, data: &[u8]) -> Vec<u8> {
        self.inner.sign(data).as_ref().to_vec()
    }
}

/// Verify a signature over `data` with `public_key`.
pub fn verify(public_key: &[u8], data: &[u8], signature: &[u8]) -> bool {
    let pk = signature::UnparsedPublicKey::new(&signature::ED25519, public_key);
    pk.verify(data, signature).is_ok()
}
