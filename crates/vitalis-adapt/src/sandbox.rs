//! The sandbox boundary. Shaped like a WASM/WASI runtime: it decides whether
//! a proposed change is permitted to run. The reference implementation is an
//! in-process [`BoundsSandbox`] bounds checker; with the `wasm-sandbox`
//! feature the same `Sandbox` trait is implemented by a real [`WasmSandbox`]
//! that executes the decision inside a `wasmtime` guest.

use crate::change::ProposedChange;

/// A boundary that decides whether a proposed change may be applied.
pub trait Sandbox {
    /// Returns `true` if the change is within the sandbox's bounds.
    fn permits(&self, change: &ProposedChange) -> bool;
}

/// The default bounds checker: a change is permitted iff its diff is within
/// the configured byte cap. (A real WASM sandbox would additionally enforce
/// capability isolation, no host IO, etc.)
#[derive(Debug, Clone, Copy)]
pub struct BoundsSandbox {
    pub max_diff_bytes: usize,
}

impl BoundsSandbox {
    pub fn new(max_diff_bytes: usize) -> Self {
        Self { max_diff_bytes }
    }
}

impl Sandbox for BoundsSandbox {
    fn permits(&self, change: &ProposedChange) -> bool {
        change.diff_size() <= self.max_diff_bytes
    }
}

#[cfg(feature = "wasm-sandbox")]
mod wasm {
    use super::Sandbox;
    use crate::change::ProposedChange;
    use std::sync::Mutex;
    use wasmtime::{Engine, Instance, Module, Store};

    // Reference guest: a change is permitted iff its diff size is within the
    // configured cap. The host passes (diff_size, cap) and the guest returns
    // 1 (allow) or 0 (deny).
    const GUEST_WAT: &str = r#"
    (module
      (func (export "permits") (param $diff i64) (param $cap i64) (result i32)
        (i64.le_s (local.get $diff) (local.get $cap))))
    "#;

    /// A sandbox whose bounds are enforced by a `wasmtime` guest module.
    pub struct WasmSandbox {
        cap: i64,
        instance: Mutex<Instance>,
        store: Mutex<Store<()>>,
    }

    impl WasmSandbox {
        /// Compile and instantiate the guest with the given byte cap.
        pub fn new(max_diff_bytes: usize) -> std::result::Result<Self, String> {
            let engine = Engine::default();
            let module = Module::new(&engine, GUEST_WAT).map_err(|e| e.to_string())?;
            let mut store = Store::new(&engine, ());
            let instance = Instance::new(&mut store, &module, &[]).map_err(|e| e.to_string())?;
            Ok(Self {
                cap: max_diff_bytes as i64,
                instance: Mutex::new(instance),
                store: Mutex::new(store),
            })
        }
    }

    impl Sandbox for WasmSandbox {
        fn permits(&self, change: &ProposedChange) -> bool {
            let diff = change.diff_size() as i64;
            let mut store = match self.store.lock() {
                Ok(s) => s,
                Err(_) => return false,
            };
            let instance = match self.instance.lock() {
                Ok(i) => i,
                Err(_) => return false,
            };
            let func = match instance.get_typed_func::<(i64, i64), i32>(&mut *store, "permits") {
                Ok(f) => f,
                Err(_) => return false,
            };
            func.call(&mut *store, (diff, self.cap))
                .map(|v| v != 0)
                .unwrap_or(false)
        }
    }
}

#[cfg(feature = "wasm-sandbox")]
pub use wasm::WasmSandbox;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::ChangeKind;

    fn change(size: usize) -> ProposedChange {
        ProposedChange::new(1, ChangeKind::Code, vec![0u8; size], "tweak")
    }

    #[test]
    fn bounds_sandbox_respects_cap() {
        let s = BoundsSandbox::new(32);
        assert!(s.permits(&change(16)));
        assert!(!s.permits(&change(64)));
    }

    #[cfg(feature = "wasm-sandbox")]
    #[test]
    fn wasm_sandbox_respects_cap() {
        let s = WasmSandbox::new(32).expect("guest should compile");
        assert!(s.permits(&change(16)));
        assert!(!s.permits(&change(64)));
    }
}
