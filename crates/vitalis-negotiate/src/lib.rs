//! # vitalis-negotiate
//!
//! The social layer: a cryptographic protocol by which agents barter compute,
//! energy, and storage. Every message is signed by its sender (Ed25519 via
//! `ring`), so a cheating peer can be detected and penalized. Settlement
//! updates each side's resource ledger, and broken bargains are recorded
//! against the peer's reputation.
//!
//! Layering: depends on `vitalis-core` only (the signing primitive is local,
//! mirroring `vitalis-defend`'s, to keep the rule that crates depend on
//! `vitalis-core` alone).

pub mod crypto;
pub mod message;
pub mod negotiator;

pub use crypto::Signer;
pub use message::{BarterMessage, SignedMessage};
pub use negotiator::{Ledger, Negotiator, Reputation};
