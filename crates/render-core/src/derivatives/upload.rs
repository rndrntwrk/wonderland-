//! Engine-neutral GPU ownership transfer for completed CPU derivatives.
//!
//! The render thread creates the concrete resource only after reserving bytes.
//! `complete` consumes that resource once; stale candidates are dropped. A
//! consumer lease pins the exact resource and its CPU artifact across resets.
use super::*;

#[derive(Clone, Copy, Debug)]
pub struct UploadLimits {
    pub max_in_flight: usize,
    pub max_entries: usize,
    pub max_bytes: u64,
}
impl Default for UploadLimits {
    fn default() -> Self {
        Self {
            max_in_flight: 4,
            max_entries: 100,
            max_bytes: 128 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UploadStats {
    pub in_flight: usize,
    pub resident_entries: usize,
    pub retained_entries: usize,
    pub pending_bytes: u64,
    pub total_bytes: u64,
    pub device_generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UploadToken {
    pub key: DerivedKey,
    pub serial: u64,
    pub device_generation: u64,
    pub source_digest: AssetKey,
    pub frame_stamp: FrameStamp,
}
struct Pending {
    key: DerivedKey,
    bytes: u64,
    generation: u64,
}
struct OwnedGpu<T> {
    resource: T,
    cpu: DerivativeLease,
    bytes: u64,
    serial: u64,
    generation: u64,
}
struct Tracked<T> {
    resource: Weak<OwnedGpu<T>>,
    bytes: u64,
}
struct UploadState<T> {
    limits: UploadLimits,
    generation: u64,
    serial: u64,
    total_bytes: u64,
    pending: BTreeMap<u64, Pending>,
    wanted: BTreeMap<DerivedKey, u64>,
    resident: BTreeMap<DerivedKey, Arc<OwnedGpu<T>>>,
    live: BTreeMap<u64, Tracked<T>>,
}
impl<T> UploadState<T> {
    fn reap(&mut self) {
        let generation = self.generation;
        self.resident
            .retain(|_, value| value.generation == generation && value.cpu.is_current());
        let mut freed = 0u64;
        self.live.retain(|_, entry| {
            let live = entry.resource.strong_count() > 0;
            if !live {
                freed += entry.bytes;
            }
            live
        });
        self.total_bytes -= freed;
    }
    fn current(&self, token: UploadToken) -> bool {
        token.device_generation == self.generation
            && self.wanted.get(&token.key) == Some(&token.serial)
            && self
                .pending
                .get(&token.serial)
                .is_some_and(|p| p.key == token.key && p.generation == token.device_generation)
    }
    fn release(&mut self, token: UploadToken) {
        if let Some(p) = self.pending.remove(&token.serial) {
            self.total_bytes -= p.bytes;
        }
        if self.wanted.get(&token.key) == Some(&token.serial) {
            self.wanted.remove(&token.key);
        }
    }
}
pub struct DerivativeGpuCache<T> {
    state: Arc<Mutex<UploadState<T>>>,
}
impl<T> Clone for DerivativeGpuCache<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}
impl<T> DerivativeGpuCache<T> {
    pub fn new(limits: UploadLimits) -> Self {
        Self {
            state: Arc::new(Mutex::new(UploadState {
                limits,
                generation: 0,
                serial: 0,
                total_bytes: 0,
                pending: BTreeMap::new(),
                wanted: BTreeMap::new(),
                resident: BTreeMap::new(),
                live: BTreeMap::new(),
            })),
        }
    }
    /// Minimum packed allocation, excluding backend alignment and staging.
    /// Callers must reserve their actual larger allocation when applicable.
    pub fn minimum_bytes(artifact: &DerivativeArtifact) -> Result<u64, DerivativeError> {
        let mut bytes = 0u64;
        for image in artifact.images() {
            bytes = bytes
                .checked_add(image.image.pixels.len() as u64 * 4)
                .ok_or(DerivativeError::Limit("GPU bytes"))?;
        }
        if let Some(geometry) = artifact.facade_geometry() {
            for mesh in [&geometry.floor, &geometry.wall] {
                bytes = bytes
                    .checked_add(mesh.vertices.len() as u64 * 32 + mesh.indices.len() as u64 * 4)
                    .ok_or(DerivativeError::Limit("GPU bytes"))?;
            }
        }
        Ok(bytes)
    }
    pub fn begin_upload(
        &self,
        cpu: DerivativeLease,
        allocation_bytes: u64,
    ) -> Result<DerivativeUploadJob<T>, DerivativeError> {
        if !cpu.is_current() {
            return Err(DerivativeError::Stale);
        }
        if allocation_bytes < Self::minimum_bytes(cpu.artifact())? {
            return Err(DerivativeError::Invalid("underreported GPU bytes"));
        }
        let mut state = self.state.lock().map_err(|_| DerivativeError::Poisoned)?;
        state.reap();
        if state.pending.len() >= state.limits.max_in_flight
            || state.pending.len() + state.live.len() >= state.limits.max_entries
        {
            return Err(DerivativeError::Limit("GPU entries"));
        }
        let total = state
            .total_bytes
            .checked_add(allocation_bytes)
            .filter(|n| *n <= state.limits.max_bytes)
            .ok_or(DerivativeError::Limit("GPU bytes"))?;
        let serial = state
            .serial
            .checked_add(1)
            .ok_or(DerivativeError::Exhausted)?;
        let token = UploadToken {
            key: cpu.artifact().key(),
            serial,
            device_generation: state.generation,
            source_digest: cpu.artifact().source_digest(),
            frame_stamp: cpu.artifact().frame_stamp(),
        };
        state.pending.insert(
            serial,
            Pending {
                key: token.key,
                bytes: allocation_bytes,
                generation: token.device_generation,
            },
        );
        state.wanted.insert(token.key, serial);
        state.serial = serial;
        state.total_bytes = total;
        Ok(DerivativeUploadJob {
            state: Arc::downgrade(&self.state),
            token,
            cpu: Some(cpu),
            bytes: allocation_bytes,
            finished: false,
        })
    }
    /// A new lifetime even when the same device/lot/content value is restored.
    /// In-flight uploads retain reservations until their handles are consumed.
    pub fn reset_device(&self) -> Result<u64, DerivativeError> {
        let mut state = self.state.lock().map_err(|_| DerivativeError::Poisoned)?;
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or(DerivativeError::Exhausted)?;
        state.resident.clear();
        state.wanted.clear();
        state.reap();
        Ok(state.generation)
    }
    pub fn acquire(
        &self,
        key: &DerivedKey,
    ) -> Result<Option<DerivativeGpuLease<T>>, DerivativeError> {
        let mut state = self.state.lock().map_err(|_| DerivativeError::Poisoned)?;
        state.reap();
        Ok(state.resident.get(key).map(|resource| DerivativeGpuLease {
            state: Arc::downgrade(&self.state),
            resource: resource.clone(),
        }))
    }
    pub fn evict(&self, key: &DerivedKey) -> Result<bool, DerivativeError> {
        let mut state = self.state.lock().map_err(|_| DerivativeError::Poisoned)?;
        let removed = state.resident.remove(key).is_some();
        state.wanted.remove(key);
        state.reap();
        Ok(removed)
    }
    pub fn stats(&self) -> Result<UploadStats, DerivativeError> {
        let mut state = self.state.lock().map_err(|_| DerivativeError::Poisoned)?;
        state.reap();
        Ok(UploadStats {
            in_flight: state.pending.len(),
            resident_entries: state.resident.len(),
            retained_entries: state.live.len() - state.resident.len(),
            pending_bytes: state.pending.values().map(|p| p.bytes).sum(),
            total_bytes: state.total_bytes,
            device_generation: state.generation,
        })
    }
}
pub struct DerivativeUploadJob<T> {
    state: Weak<Mutex<UploadState<T>>>,
    token: UploadToken,
    cpu: Option<DerivativeLease>,
    bytes: u64,
    finished: bool,
}
impl<T> DerivativeUploadJob<T> {
    pub fn token(&self) -> UploadToken {
        self.token
    }
    pub fn artifact(&self) -> &DerivativeArtifact {
        self.cpu
            .as_ref()
            .expect("owned upload CPU lease")
            .artifact()
    }
    pub fn is_cancelled(&self) -> bool {
        !self.cpu.as_ref().is_some_and(DerivativeLease::is_current)
            || !self
                .state
                .upgrade()
                .and_then(|state| state.lock().ok().map(|s| s.current(self.token)))
                .unwrap_or(false)
    }
    /// The resource is moved into one owning entry only after both CPU source
    /// and device generations are rechecked; otherwise its Drop runs here.
    pub fn complete(mut self, resource: T) -> Result<Completion, DerivativeError> {
        let Some(shared) = self.state.upgrade() else {
            return Ok(Completion::Stale);
        };
        let mut state = shared.lock().map_err(|_| DerivativeError::Poisoned)?;
        if !state.current(self.token) || !self.cpu.as_ref().is_some_and(DerivativeLease::is_current)
        {
            state.release(self.token);
            self.finished = true;
            return Ok(Completion::Stale);
        }
        let cpu = self
            .cpu
            .take()
            .ok_or(DerivativeError::Invalid("upload ownership"))?;
        let owned = Arc::new(OwnedGpu {
            resource,
            cpu,
            bytes: self.bytes,
            serial: self.token.serial,
            generation: self.token.device_generation,
        });
        // Convert the existing reservation into resident ownership; no second
        // unbudgeted allocation is admitted during the transition.
        state.pending.remove(&self.token.serial);
        state.wanted.remove(&self.token.key);
        state.live.insert(
            self.token.serial,
            Tracked {
                resource: Arc::downgrade(&owned),
                bytes: self.bytes,
            },
        );
        state.resident.insert(self.token.key, owned);
        state.reap();
        self.finished = true;
        Ok(Completion::Installed)
    }
}
impl<T> Drop for DerivativeUploadJob<T> {
    fn drop(&mut self) {
        if !self.finished {
            if let Some(state) = self.state.upgrade() {
                state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .release(self.token);
            }
        }
    }
}
pub struct DerivativeGpuLease<T> {
    state: Weak<Mutex<UploadState<T>>>,
    resource: Arc<OwnedGpu<T>>,
}
impl<T> Clone for DerivativeGpuLease<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            resource: self.resource.clone(),
        }
    }
}
impl<T> DerivativeGpuLease<T> {
    pub fn resource(&self) -> &T {
        &self.resource.resource
    }
    pub fn artifact(&self) -> &DerivativeArtifact {
        self.resource.cpu.artifact()
    }
    pub fn allocated_bytes(&self) -> u64 {
        self.resource.bytes
    }
    pub fn is_current(&self) -> bool {
        self.resource.cpu.is_current()
            && self
                .state
                .upgrade()
                .and_then(|state| {
                    state.lock().ok().map(|s| {
                        s.generation == self.resource.generation
                            && s.resident
                                .get(&self.artifact().key())
                                .is_some_and(|r| r.serial == self.resource.serial)
                    })
                })
                .unwrap_or(false)
    }
}
