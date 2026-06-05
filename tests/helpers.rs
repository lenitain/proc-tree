//! Test helper: simple HashMap-based TreeStore and CacheStore implementations.

use proc_tree::{CacheStore, PidNode, ProcInfo, TreeStore};
use std::collections::HashMap;
use std::sync::Mutex;

/// Simple HashMap-based tree store for testing.
pub struct TestTree {
    inner: Mutex<HashMap<u32, PidNode>>,
}

impl TestTree {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl TreeStore for TestTree {
    fn get_node(&self, pid: u32) -> Option<PidNode> {
        self.inner.lock().unwrap().get(&pid).cloned()
    }

    fn insert_node(&self, pid: u32, node: PidNode) {
        self.inner.lock().unwrap().insert(pid, node);
    }

    fn all_pids(&self) -> Vec<u32> {
        self.inner.lock().unwrap().keys().copied().collect()
    }
}

/// Simple HashMap-based cache store for testing.
pub struct TestCache {
    inner: Mutex<HashMap<u32, ProcInfo>>,
}

impl TestCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl CacheStore for TestCache {
    fn get_info(&self, pid: u32) -> Option<ProcInfo> {
        self.inner.lock().unwrap().get(&pid).cloned()
    }

    fn insert_info(&self, pid: u32, info: ProcInfo) {
        self.inner.lock().unwrap().insert(pid, info);
    }
}
