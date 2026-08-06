//! Persistence-across-restart integration test (proves goal **G3**).
//!
//! We write identity + state + knowledge to a file-backed store, drop it (the
//! redb file lock is released), reopen the *same* path, and assert everything
//! is intact — simulating a process crash and restart.

use tempfile::tempdir;
use vitalis_core::{AgentId, Resource, ResourceKind};
use vitalis_memory::{
    Credential, IdentityRecord, IdentityStore, Knowledge, KnowledgeKind, KnowledgeStore,
    MemoryStore, StateStore,
};

#[test]
fn state_survives_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("agent.redb");

    let id = AgentId::new();
    let cred = Credential::new("signing-seed", vec![1, 2, 3, 4]);

    // --- session 1: write everything over ONE shared connection, then drop ---
    {
        let store = MemoryStore::open(&path).unwrap();
        let ids = IdentityStore::new(store.clone());
        ids.save(&IdentityRecord::new(id, vec![cred.clone()]))
            .unwrap();

        let st = StateStore::new(store.clone());
        st.put("ledger", &Resource::new(ResourceKind::Energy, 9000.0, "J"))
            .unwrap();
        st.put("tick", &42u64).unwrap();

        let ks = KnowledgeStore::new(store.clone());
        ks.put(&Knowledge::new(
            KnowledgeKind::Profile,
            "profile",
            b"feral".to_vec(),
            1_700_000_000,
        ))
        .unwrap();
    }

    // --- session 2: reopen the same file, assert intact ---
    {
        let store = MemoryStore::open(&path).unwrap();
        let ids = IdentityStore::new(store.clone());
        let rec = ids.load(&id).unwrap().expect("identity persisted");
        assert_eq!(rec.id, id);
        assert_eq!(rec.credentials, vec![cred]);

        let st = StateStore::new(store.clone());
        let energy: Resource = st.get("ledger").unwrap().expect("ledger persisted");
        assert_eq!(energy.quantity(), 9000.0);
        let tick: u64 = st.get("tick").unwrap().expect("tick persisted");
        assert_eq!(tick, 42);

        let ks = KnowledgeStore::new(store.clone());
        let k = ks.get("profile").unwrap().expect("knowledge persisted");
        assert_eq!(k.kind, KnowledgeKind::Profile);
        assert_eq!(k.payload, b"feral");
        assert!(ks.ids().unwrap().contains(&"profile".to_string()));
    }
}

#[test]
fn knowledge_migrates_from_v1() {
    let ks = KnowledgeStore::in_memory().unwrap();
    // Hand-craft a legacy v1 envelope and stash it directly.
    ks.put_legacy_v1("fact", "old", "the host is solar-powered", 1)
        .unwrap();

    let migrated = ks.get("old").unwrap().expect("legacy record readable");
    assert_eq!(migrated.kind, KnowledgeKind::Fact);
    assert_eq!(migrated.payload, b"the host is solar-powered");
    assert_eq!(migrated.timestamp_secs, 1);
}
