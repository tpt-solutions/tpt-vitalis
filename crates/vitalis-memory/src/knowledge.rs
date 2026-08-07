//! Versioned "knowledge" blobs with forward/backward-compatible migration.
//!
//! Knowledge is the long-lived semantic state an agent carries (its profile,
//! accumulated facts, preferences). Because agents migrate and upgrade, the
//! on-disk format is versioned and migrates older records forward on read.

use crate::store::{MemoryStore, MemoryStoreError, KNOWLEDGE_TABLE};
use redb::ReadableTable;
use serde::{Deserialize, Serialize};
use std::path::Path;
use vitalis_core::{Error, Result};

/// The current knowledge schema version. Bump when the on-disk shape changes
/// and add a migration arm in `migrate`.
pub const CURRENT_SCHEMA_VERSION: u32 = 2;

/// Category of a knowledge blob.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    Profile,
    Fact,
    Preference,
    Other(String),
}

/// A decoded, current-version knowledge record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knowledge {
    pub kind: KnowledgeKind,
    pub id: String,
    pub payload: Vec<u8>,
    pub timestamp_secs: u64,
}

impl Knowledge {
    pub fn new(
        kind: KnowledgeKind,
        id: impl Into<String>,
        payload: Vec<u8>,
        timestamp_secs: u64,
    ) -> Self {
        Self {
            kind,
            id: id.into(),
            payload,
            timestamp_secs,
        }
    }
}

// ---- stored (versioned) envelopes ----------------------------------------

#[derive(Serialize, Deserialize)]
struct StoredV1 {
    kind: String,
    id: String,
    text: String,
    timestamp_secs: u64,
}

#[derive(Serialize, Deserialize)]
struct StoredV2 {
    kind: KnowledgeKind,
    id: String,
    payload: Vec<u8>,
    timestamp_secs: u64,
}

#[derive(Serialize, Deserialize)]
enum Envelope {
    V1(StoredV1),
    V2(StoredV2),
}

/// Migrate any stored envelope to the current [`Knowledge`] shape.
fn migrate(env: Envelope) -> Knowledge {
    match env {
        Envelope::V2(v) => Knowledge {
            kind: v.kind,
            id: v.id,
            payload: v.payload,
            timestamp_secs: v.timestamp_secs,
        },
        Envelope::V1(v) => {
            // v1 -> v2: textual payload becomes bytes; kind string becomes enum.
            let kind = match v.kind.as_str() {
                "profile" => KnowledgeKind::Profile,
                "fact" => KnowledgeKind::Fact,
                "preference" => KnowledgeKind::Preference,
                other => KnowledgeKind::Other(other.to_string()),
            };
            Knowledge {
                kind,
                id: v.id,
                payload: v.text.into_bytes(),
                timestamp_secs: v.timestamp_secs,
            }
        }
    }
}

/// Store and load versioned knowledge blobs.
pub struct KnowledgeStore {
    inner: MemoryStore,
}

impl KnowledgeStore {
    /// Build a knowledge store over a shared [`MemoryStore`] connection.
    pub fn new(store: MemoryStore) -> Self {
        Self { inner: store }
    }

    /// Open a knowledge store at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self::new(MemoryStore::open(path)?))
    }

    /// Open an in-memory knowledge store.
    pub fn in_memory() -> Result<Self> {
        Ok(Self::new(MemoryStore::in_memory()?))
    }

    /// Persist a knowledge record under its `id` (current schema version).
    pub fn put(&self, k: &Knowledge) -> Result<()> {
        let env = Envelope::V2(StoredV2 {
            kind: k.kind.clone(),
            id: k.id.clone(),
            payload: k.payload.clone(),
            timestamp_secs: k.timestamp_secs,
        });
        let bytes = postcard::to_stdvec(&env).map_err(|e| Error::Encode(e.to_string()))?;
        self.inner.save(KNOWLEDGE_TABLE, &k.id, &bytes)?;
        Ok(())
    }

    /// Persist raw (already-versioned) bytes under `id`. Used for migration
    /// injection and tests; normally you should use [`KnowledgeStore::put`].
    pub fn save_raw(&self, id: &str, bytes: &[u8]) -> Result<()> {
        self.inner.save(KNOWLEDGE_TABLE, id, bytes)?;
        Ok(())
    }

    /// Write a legacy (schema v1) knowledge record directly. Exposed so tests
    /// can exercise forward-migration without re-implementing the old format.
    pub fn put_legacy_v1(&self, kind: &str, id: &str, text: &str, ts: u64) -> Result<()> {
        let env = Envelope::V1(StoredV1 {
            kind: kind.to_string(),
            id: id.to_string(),
            text: text.to_string(),
            timestamp_secs: ts,
        });
        let bytes = postcard::to_stdvec(&env).map_err(|e| Error::Encode(e.to_string()))?;
        self.save_raw(id, &bytes)
    }

    /// Load and migrate a knowledge record by id.
    pub fn get(&self, id: &str) -> Result<Option<Knowledge>> {
        match self.inner.load(KNOWLEDGE_TABLE, id)? {
            Some(bytes) => {
                let env: Envelope =
                    postcard::from_bytes(&bytes).map_err(|e| Error::Encode(e.to_string()))?;
                Ok(Some(migrate(env)))
            }
            None => Ok(None),
        }
    }

    /// List the ids of all stored knowledge records.
    pub fn ids(&self) -> Result<Vec<String>> {
        let txn = self
            .inner
            .db()
            .begin_read()
            .map_err(MemoryStoreError::from)?;
        let table = txn
            .open_table(KNOWLEDGE_TABLE)
            .map_err(MemoryStoreError::from)?;
        let mut out = Vec::new();
        for entry in table.iter().map_err(MemoryStoreError::from)? {
            let (key, _) = entry.map_err(MemoryStoreError::from)?;
            out.push(key.value().to_string());
        }
        Ok(out)
    }
}
