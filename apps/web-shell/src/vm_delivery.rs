//! Ordered delivery of accepted original VM bytes across reactive render batches.
//!
//! This queue is not a VM decoder, checkpoint restorer, tick sequencer or command
//! receipt. The gateway ledger admits envelopes first; the source frame gate
//! still validates their native protocol. Distinct deliveries with equal bytes
//! must remain distinct, especially for original direct commands without IDs.
use std::collections::VecDeque;

/// Three independent generations; a reused lot ID must not revive old delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeliveryIdentity {
    pub browser_epoch: u64,
    pub source_epoch: u64,
    pub lot_incarnation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmDelivery {
    pub browser_epoch: u64,
    pub source_epoch: u64,
    pub lot_incarnation: Option<u64>,
    pub direct: bool,
    /// Exact-sized ownership makes retained payload accounting unambiguous.
    pub data: Box<[u8]>,
}

impl VmDelivery {
    fn identity(&self) -> Option<DeliveryIdentity> {
        Some(DeliveryIdentity {
            browser_epoch: self.browser_epoch,
            source_epoch: self.source_epoch,
            lot_incarnation: self.lot_incarnation.filter(|value| *value != 0)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryError {
    Unbound,
    WrongIdentity,
    EmptyPayload,
    Capacity,
    Allocation,
    NeedsReconnect,
}

/// Single-consumer FIFO, bounded in both retained payload bytes and entries.
///
/// Overload invalidates the complete pending stream: dropping an arbitrary
/// prefix and applying its suffix would silently corrupt a future replica.
/// Recovery requires a new identity or an explicit clear/rebind on reconnect.
#[derive(Debug)]
pub struct VmDeliveryQueue {
    identity: Option<DeliveryIdentity>,
    pending: VecDeque<VmDelivery>,
    bytes: usize,
    max_frames: usize,
    max_bytes: usize,
    needs_reconnect: bool,
}

impl Default for VmDeliveryQueue {
    fn default() -> Self {
        Self::with_limits(256, 64 * 1024 * 1024)
    }
}

impl VmDeliveryQueue {
    pub fn with_limits(max_frames: usize, max_bytes: usize) -> Self {
        Self {
            identity: None,
            pending: VecDeque::new(),
            bytes: 0,
            max_frames,
            max_bytes,
            needs_reconnect: false,
        }
    }

    /// Repeated session projections must not erase already queued updates or
    /// recover an overloaded stream. Zero is not an admitted lot incarnation.
    pub fn bind(&mut self, identity: Option<DeliveryIdentity>) {
        let identity = identity.filter(|value| value.lot_incarnation != 0);
        if self.identity != identity {
            self.clear();
            self.identity = identity;
        }
    }

    pub fn clear(&mut self) {
        self.pending = VecDeque::new();
        self.bytes = 0;
        self.identity = None;
        self.needs_reconnect = false;
    }

    fn invalidate(&mut self, error: DeliveryError) -> DeliveryError {
        self.pending = VecDeque::new();
        self.bytes = 0;
        self.needs_reconnect = true;
        error
    }

    pub fn push(&mut self, delivery: VmDelivery) -> Result<(), DeliveryError> {
        let identity = self.identity.ok_or(DeliveryError::Unbound)?;
        if delivery.identity() != Some(identity) {
            return Err(DeliveryError::WrongIdentity);
        }
        if self.needs_reconnect {
            return Err(DeliveryError::NeedsReconnect);
        }
        if delivery.data.is_empty() {
            return Err(self.invalidate(DeliveryError::EmptyPayload));
        }
        let bytes = match self.bytes.checked_add(delivery.data.len()) {
            Some(bytes) if bytes <= self.max_bytes && self.pending.len() < self.max_frames => bytes,
            _ => return Err(self.invalidate(DeliveryError::Capacity)),
        };
        if self.pending.try_reserve(1).is_err() {
            return Err(self.invalidate(DeliveryError::Allocation));
        }
        self.pending.push_back(delivery);
        self.bytes = bytes;
        Ok(())
    }

    pub fn pop(&mut self) -> Option<VmDelivery> {
        if self.needs_reconnect {
            return None;
        }
        let delivery = self.pending.pop_front()?;
        self.bytes -= delivery.data.len();
        Some(delivery)
    }

    pub fn pending_frames(&self) -> usize {
        self.pending.len()
    }

    pub fn pending_bytes(&self) -> usize {
        self.bytes
    }

    pub fn requires_reconnect(&self) -> bool {
        self.needs_reconnect
    }
}
