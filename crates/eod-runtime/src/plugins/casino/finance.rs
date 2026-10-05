// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Private immutable operation correlations and retained settlement obligations.
use super::*;
use crate::plugins::ProviderOperation;
use std::collections::BTreeMap;

pub(super) const PROBE_PLAYER: u8 = 1;
pub(super) const PROBE_OWNER: u8 = 2;
pub(super) const DEPOSIT: u8 = 3;
pub(super) const WITHDRAW: u8 = 4;
pub(super) const BET: u8 = 5;
pub(super) const DOUBLE: u8 = 6;
pub(super) const SPLIT: u8 = 7;
pub(super) const INSURANCE: u8 = 8;
pub(super) const CALL: u8 = 9;
pub(super) const PAYOUT: u8 = 10;
pub(super) const SIDE_PAYOUT: u8 = 11;
pub(super) const REFUND: u8 = 12;
const MAX_PENDING: usize = 64;

#[derive(Clone)]
pub(super) struct Pending {
    pub kind: u8,
    pub seat: u8,
    pub primary: u32,
    pub secondary: u32,
    pub operation: Operation,
}
impl Pending {
    fn valid(&self) -> bool {
        (1..=12).contains(&self.kind)
            && self.seat < 16
            && self.primary <= i32::MAX as u32
            && self.secondary <= i32::MAX as u32
            && self.operation.valid()
    }
    fn save(&self, w: &mut Writer) {
        w.u8(self.kind);
        w.u8(self.seat);
        w.u32(self.primary);
        w.u32(self.secondary);
        self.operation.save(w)
    }
    fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let p = Self {
            kind: r.u8()?,
            seat: r.u8()?,
            primary: r.u32()?,
            secondary: r.u32()?,
            operation: Operation::restore(r)?,
        };
        if !p.valid() {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(p)
    }
}
#[derive(Clone)]
pub(super) struct Finance {
    next: u64,
    pub pending: BTreeMap<u64, Pending>,
    pub denied: Vec<Pending>,
}
pub(super) struct Receipt {
    pub success: bool,
    pub source_balance: Option<u32>,
    pub target_balance: Option<u32>,
}
impl Finance {
    pub fn new() -> Self {
        Self {
            next: 1,
            pending: BTreeMap::new(),
            denied: Vec::new(),
        }
    }
    pub fn request(&mut self, p: Pending, out: &mut Actions) -> Result<(), Error> {
        if !p.valid() {
            return Err(Error::InvalidPluginInput);
        }
        if self.pending.len() + self.denied.len() >= MAX_PENDING {
            return Err(Error::EffectLimit);
        }
        let id = self.next;
        let next = id
            .checked_add(1)
            .filter(|n| *n != u64::MAX)
            .ok_or(Error::CounterExhausted)?;
        out.provider(id, ProviderOperation::Casino(p.operation.clone()));
        self.pending.insert(id, p);
        self.next = next;
        Ok(())
    }
    pub fn complete(&mut self, id: u64, reply: &Reply) -> Result<(Pending, Receipt), Error> {
        let p = self.pending.get(&id).ok_or(Error::UnknownEffect)?;
        let receipt = match reply {
            Reply::ProviderDenied => Receipt {
                success: false,
                source_balance: None,
                target_balance: None,
            },
            Reply::Transaction {
                success,
                source,
                target,
                amount,
                source_balance,
                target_balance,
            } => {
                if p.operation.details() != (*source, *target, *amount)
                    || (!matches!(source, Account::System) && *source_balance > i32::MAX as u32)
                    || (!matches!(target, Account::System) && *target_balance > i32::MAX as u32)
                {
                    return Err(Error::ProviderReceiptMismatch);
                }
                Receipt {
                    success: *success,
                    source_balance: Some(*source_balance),
                    target_balance: Some(*target_balance),
                }
            }
        };
        Ok((
            self.pending.remove(&id).ok_or(Error::UnknownEffect)?,
            receipt,
        ))
    }
    pub fn retain_denied(&mut self, p: Pending) -> Result<(), Error> {
        if p.kind < PAYOUT || self.pending.len() + self.denied.len() >= MAX_PENDING {
            return Err(Error::EffectLimit);
        }
        self.denied.push(p);
        Ok(())
    }
    pub fn retry(&mut self, out: &mut Actions) -> Result<(), Error> {
        let due = std::mem::take(&mut self.denied);
        for p in due {
            self.request(p, out)?;
        }
        Ok(())
    }
    pub fn owes(&self) -> bool {
        !self.denied.is_empty() || self.pending.values().any(|p| p.kind >= PAYOUT)
    }
    pub fn busy(&self) -> bool {
        !self.pending.is_empty() || !self.denied.is_empty()
    }
    pub fn player_busy(&self, seat: u8) -> bool {
        self.pending
            .values()
            .chain(self.denied.iter())
            .any(|p| p.seat == seat)
    }
    pub fn save(&self, w: &mut Writer) {
        w.u64(self.next);
        w.u32(self.pending.len() as u32);
        for (k, p) in &self.pending {
            w.u64(*k);
            p.save(w)
        }
        w.u32(self.denied.len() as u32);
        for p in &self.denied {
            p.save(w)
        }
    }
    pub fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let next = r.u64()?;
        if next == 0 || next == u64::MAX {
            return Err(Error::InvalidCheckpoint);
        }
        let n = r.count(MAX_PENDING)?;
        let mut pending = BTreeMap::new();
        for _ in 0..n {
            let id = r.u64()?;
            if id == 0 || id >= next || pending.contains_key(&id) {
                return Err(Error::InvalidCheckpoint);
            }
            pending.insert(id, Pending::restore(r)?);
        }
        let n = r.count(MAX_PENDING - pending.len())?;
        let mut denied = Vec::with_capacity(n);
        for _ in 0..n {
            let p = Pending::restore(r)?;
            if p.kind < PAYOUT {
                return Err(Error::InvalidCheckpoint);
            }
            denied.push(p)
        }
        Ok(Self {
            next,
            pending,
            denied,
        })
    }
}
