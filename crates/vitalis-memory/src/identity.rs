//! Identity persistence: bind an [`AgentId`] to durable credentials so the
//! agent can re-establish who it is across restarts and migrations.

use crate::store::{MemoryStore, IDENTITY_TABLE};
use serde::{Deserialize, Serialize};
use std::path::Path;
use vitalis_core::{AgentId, Error, Result};

/// A credential bound to an identity. Opaque to memory — signing/verification
/// lives in `vitalis-defend` / `vitalis-negotiate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    /// Human label for what this credential unlocks.
    pub label: String,
    /// Opaque credential bytes (e.g. a signing seed or token).
    pub payload: Vec<u8>,
}

impl Credential {
    pub fn new(label: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            label: label.into(),
            payload,
        }
    }
}

/// A persisted identity: an agent plus its credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityRecord {
    pub id: AgentId,
    pub credentials: Vec<Credential>,
}

impl IdentityRecord {
    pub fn new(id: AgentId, credentials: Vec<Credential>) -> Self {
        Self { id, credentials }
    }
}

/// Store and load [`IdentityRecord`]s keyed by [`AgentId`].
pub struct IdentityStore {
    inner: MemoryStore,
}

impl IdentityStore {
    /// Build an identity store over a shared [`MemoryStore`] connection.
    pub fn new(store: MemoryStore) -> Self {
        Self { inner: store }
    }

    /// Open an identity store at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self::new(MemoryStore::open(path)?))
    }

    /// Open an in-memory identity store.
    pub fn in_memory() -> Result<Self> {
        Ok(Self {
            inner: MemoryStore::in_memory()?,
        })
    }

    /// Persist (or overwrite) an identity record.
    pub fn save(&self, record: &IdentityRecord) -> Result<()> {
        let key = record.id.to_string();
        let bytes = postcard::to_stdvec(record).map_err(|e| Error::Encode(e.to_string()))?;
        self.inner.save(IDENTITY_TABLE, &key, &bytes)?;
        Ok(())
    }

    /// Load an identity by id, if previously persisted.
    pub fn load(&self, id: &AgentId) -> Result<Option<IdentityRecord>> {
        let key = id.to_string();
        match self.inner.load(IDENTITY_TABLE, &key)? {
            Some(bytes) => {
                let rec: IdentityRecord =
                    postcard::from_bytes(&bytes).map_err(|e| Error::Encode(e.to_string()))?;
                Ok(Some(rec))
            }
            None => Ok(None),
        }
    }
}
