//! Bounded, single-consumer inbox between the socket and the world adapter.
//!
//! A reactive signal is a notification, not a message queue. Keep every admitted
//! delivery here until the consumer takes it. Never evict a prefix, deduplicate
//! equal source bytes, or keep playing after overflow. Reconnect is required
//! after a buffer failure; the last visible world must not be labelled current.

/// Buffer limits are memory/backlog admission limits, not player or lot limits.
const MAX_FRAMES: usize = 4096;
const MAX_RETAINED_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmStream {
    pub browser_epoch: u64,
    pub source_epoch: u64,
    pub lot_incarnation: u64,
}
impl VmStream {
    fn valid(self) -> bool {
        self.browser_epoch != 0 && self.source_epoch != 0 && self.lot_incarnation != 0
    }
}

#[derive(Clone, Debug)]
pub struct VmDelivery {
    pub browser_epoch: u64,
    pub source_epoch: u64,
    pub lot_incarnation: Option<u64>,
    pub direct: bool,
    pub data: Vec<u8>,
}
impl VmDelivery {
    fn stream(&self) -> Option<VmStream> {
        Some(VmStream {
            browser_epoch: self.browser_epoch,
            source_epoch: self.source_epoch,
            lot_incarnation: self.lot_incarnation?,
        })
        .filter(|stream| stream.valid())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InboxLimits {
    pub max_frames: usize,
    pub max_retained_bytes: usize,
}
impl Default for InboxLimits {
    fn default() -> Self {
        Self {
            max_frames: 256,
            max_retained_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboxError {
    InvalidLimits,
    WrongStream,
    FrameLimit,
    ByteLimit,
    Allocation,
    RecoveryRequired,
}

pub struct VmInbox {
    stream: Option<VmStream>,
    pending: Vec<VmDelivery>,
    retained_bytes: usize,
    recovery_required: bool,
    limits: InboxLimits,
}
impl Default for VmInbox {
    fn default() -> Self {
        Self::new(InboxLimits::default()).expect("constant inbox limits are valid")
    }
}
impl VmInbox {
    pub fn new(limits: InboxLimits) -> Result<Self, InboxError> {
        if limits.max_frames == 0
            || limits.max_frames > MAX_FRAMES
            || limits.max_retained_bytes == 0
            || limits.max_retained_bytes > MAX_RETAINED_BYTES
        {
            return Err(InboxError::InvalidLimits);
        }
        Ok(Self {
            stream: None,
            pending: Vec::new(),
            retained_bytes: 0,
            recovery_required: false,
            limits,
        })
    }

    /// Bind only an authenticated, admitted source projection. A duplicate
    /// status update for the same stream must not erase pending deliveries or
    /// unlock a failed stream. Socket replacement uses `reset` explicitly.
    pub fn bind(&mut self, stream: Option<VmStream>) {
        let stream = stream.filter(|stream| stream.valid());
        if self.stream != stream {
            self.reset();
            self.stream = stream;
        }
    }

    /// Drop both payloads and their retained allocation on logout, reconnect,
    /// source replacement or owner cleanup. This does not authorize a stream.
    pub fn reset(&mut self) {
        self.stream = None;
        self.pending = Vec::new();
        self.retained_bytes = 0;
        self.recovery_required = false;
    }

    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    fn fail(&mut self, error: InboxError) -> Result<(), InboxError> {
        self.pending = Vec::new();
        self.retained_bytes = 0;
        self.recovery_required = true;
        Err(error)
    }

    pub fn push(&mut self, frame: VmDelivery) -> Result<(), InboxError> {
        if self.stream.is_none() || frame.stream() != self.stream {
            return Err(InboxError::WrongStream);
        }
        if self.recovery_required {
            return Err(InboxError::RecoveryRequired);
        }
        if self.pending.len() >= self.limits.max_frames {
            return self.fail(InboxError::FrameLimit);
        }
        let Some(retained_bytes) = self
            .retained_bytes
            .checked_add(frame.data.capacity())
            .filter(|total| *total <= self.limits.max_retained_bytes)
        else {
            return self.fail(InboxError::ByteLimit);
        };
        // Reserve exact growth; don't retain an unaccounted extra frame array.
        if self.pending.try_reserve_exact(1).is_err() {
            return self.fail(InboxError::Allocation);
        }
        self.pending.push(frame);
        self.retained_bytes = retained_bytes;
        Ok(())
    }

    /// Moves the backlog to its one consumer without copying any VM payload.
    /// Checking the scope first prevents a stale view from draining a new lot.
    pub fn drain(&mut self, stream: VmStream) -> Result<Vec<VmDelivery>, InboxError> {
        if !stream.valid() || self.stream != Some(stream) {
            return Err(InboxError::WrongStream);
        }
        if self.recovery_required {
            return Err(InboxError::RecoveryRequired);
        }
        self.retained_bytes = 0;
        Ok(std::mem::take(&mut self.pending))
    }
}
