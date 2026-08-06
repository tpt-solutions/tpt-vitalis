//! Generic, checkpoint-able key/value state persistence.
//!
//! `StateStore` persists any `Serialize`/`DeserializeOwned` value under a
//! string key. This is the mechanism an agent uses to checkpoint its own
//! working state so it can resume after a restart or migrate to another node.

use crate::store::{MemoryStore, STATE_TABLE};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::Path;
use vitalis_core::{Error, Result};

/// Generic durable key/value state store.
pub struct StateStore {
    inner: MemoryStore,
}

impl StateStore {
    /// Build a state store over a shared [`MemoryStore`] connection.
    pub fn new(store: MemoryStore) -> Self {
        Self { inner: store }
    }

    /// Open a state store at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self::new(MemoryStore::open(path)?))
    }

    /// Open an in-memory state store.
    pub fn in_memory() -> Result<Self> {
        Ok(Self {
            inner: MemoryStore::in_memory()?,
        })
    }

    /// Persist `value` under `key`.
    pub fn put<V: Serialize>(&self, key: &str, value: &V) -> Result<()> {
        let bytes = postcard::to_stdvec(value).map_err(|e| Error::Encode(e.to_string()))?;
        self.inner.save(STATE_TABLE, key, &bytes)?;
        Ok(())
    }

    /// Load `key`, if present, deserializing into `V`.
    pub fn get<V: DeserializeOwned>(&self, key: &str) -> Result<Option<V>> {
        match self.inner.load(STATE_TABLE, key)? {
            Some(bytes) => {
                let v: V =
                    postcard::from_bytes(&bytes).map_err(|e| Error::Encode(e.to_string()))?;
                Ok(Some(v))
            }
            None => Ok(None),
        }
    }

    /// Remove `key`.
    pub fn remove(&self, key: &str) -> Result<()> {
        self.inner.delete(STATE_TABLE, key)?;
        Ok(())
    }

    /// True if `key` exists.
    pub fn contains(&self, key: &str) -> Result<bool> {
        Ok(self.inner.load(STATE_TABLE, key)?.is_some())
    }
}
