//! Capability sandbox for untrusted extensions.
//!
//! The reference implementation is an in-process [`NullSandbox`] (permits
//! everything; the real boundary is the caller's policy). With the
//! `wasm-sandbox` feature, [`WasmSandbox`] executes capability decisions inside
//! a real `wasmtime` guest — this is the WASM/WASI runtime referenced in the
//! design doc, kept behind a feature so the default build stays dependency-light.

/// Decides whether an untrusted extension action is permitted.
pub trait CapabilitySandbox {
    /// `requested_access` is a verb like `"read"`/`"write"`/`"exec"`/`"net_bind"`;
    /// `target` is a domain like `"fs"`/`"net"`/`"mem"`. Returns `true` if the
    /// action is allowed.
    fn permits(&self, requested_access: &str, target: &str) -> bool;
}

/// A no-op sandbox that permits everything. Default when `wasm-sandbox` is off.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSandbox;

impl CapabilitySandbox for NullSandbox {
    fn permits(&self, _requested_access: &str, _target: &str) -> bool {
        true
    }
}

#[cfg(feature = "wasm-sandbox")]
mod guest {
    use super::CapabilitySandbox;
    use std::sync::Mutex;
    use wasmtime::{Engine, Instance, Module, Store};

    // Reference guest policy: deny filesystem writes and any network bind;
    // allow everything else. Actions/targets are mapped to small ids by the
    // host before calling into the guest.
    const GUEST_WAT: &str = r#"
    (module
      (func (export "permits") (param $action i64) (param $target i64) (result i32)
        ;; deny = (action == write || action == net_bind)
        ;;        && (target == fs || target == net)
        (i32.sub
          (i32.const 1)
          (i32.and
            (i32.or
              (i32.eq (i32.wrap_i64 (local.get $action)) (i32.const 1))
              (i32.eq (i32.wrap_i64 (local.get $action)) (i32.const 5)))
            (i32.or
              (i64.eq (local.get $target) (i64.const 2))
              (i64.eq (local.get $target) (i64.const 3)))))))
    "#;

    /// A capability sandbox implemented as a `wasmtime` guest module.
    pub struct WasmSandbox {
        instance: Mutex<Instance>,
        store: Mutex<Store<()>>,
    }

    fn hash_access(a: &str) -> i64 {
        match a {
            "read" => 0,
            "write" => 1,
            "exec" => 4,
            "net_bind" => 5,
            _ => 9,
        }
    }

    fn hash_target(t: &str) -> i64 {
        match t {
            "fs" => 2,
            "net" => 3,
            "mem" => 6,
            _ => 9,
        }
    }

    impl WasmSandbox {
        /// Compile and instantiate the guest. Fails only if the runtime or the
        /// embedded guest is unavailable.
        pub fn try_new() -> std::result::Result<Self, String> {
            let engine = Engine::default();
            let module = Module::new(&engine, GUEST_WAT).map_err(|e| e.to_string())?;
            let mut store = Store::new(&engine, ());
            let instance = Instance::new(&mut store, &module, &[]).map_err(|e| e.to_string())?;
            Ok(Self {
                instance: Mutex::new(instance),
                store: Mutex::new(store),
            })
        }
    }

    impl CapabilitySandbox for WasmSandbox {
        fn permits(&self, requested_access: &str, target: &str) -> bool {
            let action = hash_access(requested_access);
            let target_id = hash_target(target);
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
            func.call(&mut *store, (action, target_id))
                .map(|v| v != 0)
                .unwrap_or(false)
        }
    }
}

#[cfg(feature = "wasm-sandbox")]
pub use guest::WasmSandbox;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_sandbox_permits_all() {
        let s = NullSandbox;
        assert!(s.permits("write", "fs"));
        assert!(s.permits("read", "mem"));
    }

    #[cfg(feature = "wasm-sandbox")]
    #[test]
    fn wasm_sandbox_denies_fs_write_and_net_bind() {
        let s = WasmSandbox::try_new().expect("guest should compile");
        assert!(s.permits("read", "fs"));
        assert!(!s.permits("write", "fs"));
        assert!(!s.permits("net_bind", "net"));
        assert!(s.permits("exec", "mem"));
    }
}
