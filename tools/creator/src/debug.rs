// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, obtain one at https://mozilla.org/MPL/2.0/.
//! A real provider must supply isolated snapshots and observations. No VM is simulated by this tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugSnapshot {
    pub tick: u64,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Watch {
    pub entity: u64,
    pub field: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchValue {
    pub watch: Watch,
    pub value: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceEvent {
    pub tick: u64,
    pub entity: u64,
    pub routine: u16,
    pub instruction: usize,
    pub opcode: u16,
}
/// Providers must only observe or advance an isolated copy. Integration owns provider trust and VM capability checks.
pub trait IsolatedDebugProvider {
    fn inspect(
        &self,
        snapshot: &DebugSnapshot,
        watches: &[Watch],
    ) -> Result<Vec<WatchValue>, String>;
    fn trace(&self, snapshot: &DebugSnapshot) -> Result<Vec<TraceEvent>, String>;
    fn step_isolated(&self, _snapshot: &DebugSnapshot) -> Result<DebugSnapshot, String> {
        Err("isolated VM stepping unsupported: no capable VM provider installed".into())
    }
}
pub struct UnsupportedDebugProvider;
impl IsolatedDebugProvider for UnsupportedDebugProvider {
    fn inspect(&self, _: &DebugSnapshot, _: &[Watch]) -> Result<Vec<WatchValue>, String> {
        Err("watch evaluation unsupported: no isolated VM provider installed".into())
    }
    fn trace(&self, _: &DebugSnapshot) -> Result<Vec<TraceEvent>, String> {
        Err("trace evaluation unsupported: no isolated VM provider installed".into())
    }
}
