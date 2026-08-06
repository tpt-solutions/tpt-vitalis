//! Low-level embedded KV store abstraction over [redb].
//!
//! [redb]: https://docs.rs/redb

use redb::{Database, StorageError, TableDefinition, TableError, TransactionError};
use std::path::Path;
use std::sync::Arc;

/// Table of persisted agent identities (keyed by `AgentId` hex).
pub const IDENTITY_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vitalis_identity");
/// Table of generic checkpoint-able agent state.
pub const STATE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vitalis_state");
/// Table of versioned knowledge blobs.
pub const KNOWLEDGE_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("vitalis_knowledge");

/// Error type for the memory store layer.
#[derive(Debug)]
pub enum MemoryStoreError {
    /// Underlying redb failure.
    Backend(Box<dyn std::error::Error + Send + Sync>),
    /// Serialization / deserialization failure.
    Encode(String),
    /// Caller-supplied I/O / validation failure.
    Invalid(String),
}

impl std::fmt::Display for MemoryStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryStoreError::Backend(e) => write!(f, "memory backend error: {e}"),
            MemoryStoreError::Encode(m) => write!(f, "memory encode error: {m}"),
            MemoryStoreError::Invalid(m) => write!(f, "memory invalid: {m}"),
        }
    }
}

impl std::error::Error for MemoryStoreError {}

impl From<redb::DatabaseError> for MemoryStoreError {
    fn from(e: redb::DatabaseError) -> Self {
        MemoryStoreError::Backend(Box::new(e))
    }
}
impl From<TransactionError> for MemoryStoreError {
    fn from(e: TransactionError) -> Self {
        MemoryStoreError::Backend(Box::new(e))
    }
}
impl From<TableError> for MemoryStoreError {
    fn from(e: TableError) -> Self {
        MemoryStoreError::Backend(Box::new(e))
    }
}
impl From<StorageError> for MemoryStoreError {
    fn from(e: StorageError) -> Self {
        MemoryStoreError::Backend(Box::new(e))
    }
}
impl From<redb::CommitError> for MemoryStoreError {
    fn from(e: redb::CommitError) -> Self {
        MemoryStoreError::Backend(Box::new(e))
    }
}
impl From<std::io::Error> for MemoryStoreError {
    fn from(e: std::io::Error) -> Self {
        MemoryStoreError::Backend(Box::new(e))
    }
}

impl From<MemoryStoreError> for vitalis_core::Error {
    fn from(e: MemoryStoreError) -> Self {
        match e {
            MemoryStoreError::Backend(b) => vitalis_core::Error::Other(b),
            MemoryStoreError::Encode(m) => vitalis_core::Error::Encode(m),
            MemoryStoreError::Invalid(m) => vitalis_core::Error::Invalid(m),
        }
    }
}

type R<T> = std::result::Result<T, MemoryStoreError>;

/// A transactional, embedded key/value store.
///
/// The underlying redb `Database` is reference-counted, so multiple logical
/// stores (identity, state, knowledge) can share **one** open connection to a
/// file without hitting redb's single-open-per-path guard. Durability follows
/// redb's fsync-at-commit semantics: a committed `WriteTransaction` is durably
/// persisted, so a process restart that reopens the same path sees the data
/// (proving **G3**).
#[derive(Clone)]
pub struct MemoryStore {
    db: Arc<Database>,
}

impl MemoryStore {
    /// Open (or create) a store at the given file path.
    pub fn open(path: &Path) -> R<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        Ok(Self {
            db: Arc::new(Database::create(path)?),
        })
    }

    /// Open a throwaway in-memory store (useful for tests / ephemeral agents).
    pub fn in_memory() -> R<Self> {
        let backend = redb::backends::InMemoryBackend::new();
        Ok(Self {
            db: Arc::new(redb::Database::builder().create_with_backend(backend)?),
        })
    }

    /// Persist `value` under `key` in `table`, durably.
    pub fn save(&self, table: TableDefinition<&str, &[u8]>, key: &str, value: &[u8]) -> R<()> {
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(table)?;
            t.insert(key, value)?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Load `key` from `table`, if present.
    pub fn load(&self, table: TableDefinition<&str, &[u8]>, key: &str) -> R<Option<Vec<u8>>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(table)?;
        Ok(t.get(key)?.map(|g| g.value().to_vec()))
    }

    /// Delete `key` from `table`.
    pub fn delete(&self, table: TableDefinition<&str, &[u8]>, key: &str) -> R<()> {
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(table)?;
            t.remove(key)?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Access the underlying database (e.g. for multi-table transactions).
    pub fn db(&self) -> &Database {
        &self.db
    }
}
