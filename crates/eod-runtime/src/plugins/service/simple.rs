// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Newspaper, bulletin, property, cooldown and trunk source handlers.
use super::{types::*, wire::*};
use crate::{
    plugins::{
        ProviderOperation,
        codec::{Reader, Writer},
        common::*,
    },
    *,
};
use std::collections::BTreeMap;
const MAX_DATETIME: i64 = 3_155_378_975_999_999_999;
const TICKS_PER_SECOND: i64 = 10_000_000;
#[derive(Clone)]
pub(crate) struct Simple {
    kind: u8,
    object: u32,
    lot: u32,
    guid: u32,
    category: u8,
    community: bool,
    now: i64,
    value: u32,
    shown: bool,
    posted: bool,
    anim: bool,
    next_anim: Option<i16>,
    collection: Option<Vec<u64>>,
    last_news: Option<Vec<u8>>,
    last_bulletin: Option<(u32, u32)>,
    next: u64,
    pending: BTreeMap<u64, Pending>,
}
#[derive(Clone)]
struct Pending {
    seat: Option<u8>,
    avatar: u32,
    kind: u8,
    mode: i16,
    duration: i64,
    now: i64,
}
impl Simple {
    pub(super) fn new(c: Config) -> Result<Self, Error> {
        let mut s = Self {
            kind: 0,
            object: 0,
            lot: 0,
            guid: 0,
            category: 0,
            community: false,
            now: 0,
            value: 0,
            shown: false,
            posted: false,
            anim: false,
            next_anim: None,
            collection: None,
            last_news: None,
            last_bulletin: None,
            next: 1,
            pending: BTreeMap::new(),
        };
        match c {
            Config::Newspaper => {}
            Config::Bulletin { lot } => {
                if lot == 0 {
                    return Err(Error::InvalidPluginInput);
                }
                s.kind = 1;
                s.lot = lot
            }
            Config::PropertySelect => s.kind = 2,
            Config::Cooldown {
                persistent_object,
                object_guid,
                lot,
                category,
                community,
                utc_ticks,
            } => {
                if persistent_object == 0
                    || object_guid == 0
                    || lot == 0
                    || !(0..=MAX_DATETIME).contains(&utc_ticks)
                {
                    return Err(Error::InvalidPluginInput);
                }
                s.kind = 3;
                s.object = persistent_object;
                s.guid = object_guid;
                s.lot = lot;
                s.category = category;
                s.community = community;
                s.now = utc_ticks
            }
            Config::Trunk { kind } => {
                if kind > 8 {
                    return Err(Error::InvalidPluginInput);
                }
                s.kind = 4;
                s.category = kind
            }
            _ => return Err(Error::WrongPlugin),
        }
        Ok(s)
    }
    pub(super) fn trusted_object(&self) -> Option<u32> {
        (self.kind == 3).then_some(self.object)
    }
    pub(super) fn plugin(&self) -> PluginId {
        PluginId(match self.kind {
            0 => 0x1000,
            1 => 0x1003,
            2 => 0x2000,
            3 => 0x1004,
            4 => 0xAA5E36DC,
            _ => unreachable!(),
        })
    }
    fn queue(
        &mut self,
        a: &mut Actions,
        operation: Operation,
        pending: Pending,
    ) -> Result<(), Error> {
        if self.pending.len() >= 4 {
            return Err(Error::PersistencePending);
        }
        let callback = self.next;
        self.next = self.next.checked_add(1).ok_or(Error::CounterExhausted)?;
        self.pending.insert(callback, pending);
        a.provider(callback, ProviderOperation::Service(operation));
        Ok(())
    }
    pub(super) fn start(&mut self, _: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if self.kind == 1 {
            self.queue(
                &mut a,
                Operation::BulletinState { lot: self.lot },
                Pending {
                    seat: None,
                    avatar: 0,
                    kind: 1,
                    mode: 0,
                    duration: 0,
                    now: 0,
                },
            )?
        }
        Ok(a)
    }
    pub(super) fn join(&mut self, m: Member, roster: &Roster, _: u64) -> Result<Actions, Error> {
        if roster.iter().flatten().count() > 1 {
            return Err(Error::ParticipantLimit);
        }
        let mut a = Actions::default();
        let mut p = Pending {
            seat: Some(m.seat),
            avatar: m.avatar_id,
            kind: self.kind,
            mode: 0,
            duration: 0,
            now: 0,
        };
        match self.kind {
            0 => {
                a.text(Target::Member(m.seat), "newspaper_show", String::new());
                self.queue(&mut a, Operation::DynamicPayouts, p)?
            }
            1 => {
                a.text(
                    Target::Member(m.seat),
                    "bulletin_show",
                    m.input.registers[0].to_string(),
                );
            }
            2 => {
                self.value = ((i32::from(m.input.registers[1]) << 16)
                    | i32::from(m.input.registers[0])) as u32;
                self.shown = false
            }
            3 => {
                let r = m.input.registers;
                let mode = r[0];
                if !(0..=7).contains(&mode) {
                    return Err(Error::InvalidPluginInput);
                }
                let duration = (i64::from(r[1]) * 3600 + i64::from(r[2]) * 60 + i64::from(r[3]))
                    * TICKS_PER_SECOND;
                self.now
                    .checked_add(duration)
                    .filter(|v| (0..=MAX_DATETIME).contains(v))
                    .ok_or(Error::InvalidPluginInput)?;
                p.mode = mode;
                p.duration = duration;
                p.now = self.now;
                let local = mode <= 1 || (self.community && matches!(mode, 5 | 7));
                let by_account = matches!(mode, 1 | 3 | 6 | 7);
                let category = if !local && mode >= 4 {
                    Some(self.category)
                } else {
                    None
                };
                self.queue(
                    &mut a,
                    Operation::Cooldown {
                        object: self.object,
                        object_guid: self.guid,
                        lot: self.lot,
                        category,
                        local,
                        by_account,
                        avatar: m.avatar_id,
                        now_ticks: self.now,
                        duration_ticks: duration,
                    },
                    p,
                )?;
            }
            4 => {
                self.value = u32::from(m.input.gender != 0);
                a.text(
                    Target::Member(m.seat),
                    "trunk_fill_UI",
                    self.collection_path(),
                );
                self.queue(
                    &mut a,
                    Operation::ReadTrunkCollection {
                        kind: self.category,
                        gender: self.value as u8,
                    },
                    p,
                )?
            }
            _ => unreachable!(),
        }
        Ok(a)
    }
    fn collection_path(&self) -> String {
        let names = [
            "wedding", "scifi", "vegas", "vaudwest", "uniforms", "costumes", "oldworld", "sports",
            "toga",
        ];
        format!(
            "{}_{}male.col",
            names[self.category as usize],
            if self.value == 0 { "" } else { "fe" }
        )
    }
    pub(super) fn allows(&self, event: &str, binary: bool) -> bool {
        match self.kind {
            0 => !binary && event == "close",
            1 => !binary && matches!(event, "close" | "bulletin_mode" | "bulletin_posted"),
            2 => (!binary && event == "close") || (binary && event == "property_select"),
            3 => false,
            4 => !binary && matches!(event, "trunk_close_UI" | "trunk_wear_costume"),
            _ => false,
        }
    }
    pub(super) fn message(
        &mut self,
        m: Member,
        event: &str,
        bytes: &[u8],
        _: &Roster,
        _: u64,
    ) -> Result<Actions, Error> {
        let mut a = Actions::default();
        match (self.kind, event) {
            (_, "close") | (_, "trunk_close_UI") => a.close(Target::All),
            (1, "bulletin_mode") => {
                if let Some(code) = parse_i32(bytes).filter(|c| (0..3).contains(c)) {
                    let code = code as i16 + 2;
                    if self.anim {
                        self.next_anim = Some(code)
                    } else {
                        a.object(Target::Member(m.seat), code, vec![]);
                        self.anim = true
                    }
                }
            }
            (1, "bulletin_posted") => {
                if !self.posted {
                    self.posted = true;
                    self.queue(
                        &mut a,
                        Operation::BulletinState { lot: self.lot },
                        Pending {
                            seat: Some(m.seat),
                            avatar: m.avatar_id,
                            kind: 1,
                            mode: 0,
                            duration: 0,
                            now: 0,
                        },
                    )?
                }
            }
            (2, "property_select") => {
                if (4..=132).contains(&bytes.len()) {
                    self.value = u32::from_le_bytes(
                        bytes[..4].try_into().map_err(|_| Error::InvalidMessage)?,
                    );
                    a.object(
                        Target::Member(m.seat),
                        1,
                        vec![self.value as i16, (self.value >> 16) as i16],
                    );
                    for b in &bytes[4..] {
                        a.object(Target::Member(m.seat), 2, vec![i16::from(*b)])
                    }
                }
            }
            (4, "trunk_wear_costume") => {
                let collection = self.collection.as_ref().ok_or(Error::PluginNotReady)?;
                let selected = parse_u64(bytes).filter(|id| collection.contains(id)).or(
                    if self.category == 5 {
                        Some(6_000_069_312_525)
                    } else {
                        None
                    },
                );
                if let Some(outfit) = selected {
                    a.command(NativeCommand::SetOutfit {
                        avatar_id: m.avatar_id,
                        scope: OutfitScope::DynamicCostume,
                        outfit,
                    });
                    a.object(Target::Member(m.seat), 1, vec![]);
                    a.close(Target::All)
                }
            }
            _ => return Err(Error::EventNotAllowed),
        }
        Ok(a)
    }
    pub(super) fn tick(&mut self, roster: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if self.kind == 2
            && !self.shown
            && let Some(m) = roster.iter().flatten().next()
        {
            a.binary(
                Target::Member(m.seat),
                "property_show",
                self.value.to_le_bytes().to_vec(),
            );
            self.shown = true
        }
        Ok(a)
    }
    pub(super) fn vm_event(
        &mut self,
        input: &VmInput,
        roster: &Roster,
        _: u64,
    ) -> Result<Actions, Error> {
        let mut a = Actions::default();
        match input {
            VmInput::BulletinAnimationFinished { seat } if self.kind == 1 => {
                if !roster.get(*seat as usize).is_some_and(Option::is_some) {
                    return Err(Error::InvalidPluginInput);
                }
                if let Some(code) = self.next_anim.take() {
                    a.object(Target::Member(*seat), code, vec![])
                } else {
                    self.anim = false
                }
            }
            VmInput::SetUtcTicks(now) if self.kind == 3 => {
                if !(0..=MAX_DATETIME).contains(now) || *now < self.now {
                    return Err(Error::InvalidPluginInput);
                }
                self.now = *now
            }
            _ => return Err(Error::WrongPlugin),
        }
        Ok(a)
    }
    pub(super) fn reply(
        &mut self,
        callback: u64,
        reply: &Reply,
        roster: &Roster,
        _: u64,
    ) -> Result<Actions, Error> {
        let p = self
            .pending
            .get(&callback)
            .cloned()
            .ok_or(Error::ProviderReceiptMismatch)?;
        let mut a = Actions::default();
        let target = match p.seat {
            Some(seat) => {
                if roster
                    .get(seat as usize)
                    .and_then(|v| *v)
                    .is_none_or(|m| m.avatar_id != p.avatar)
                {
                    self.pending.remove(&callback);
                    return Ok(a);
                }
                Target::Member(seat)
            }
            None => Target::Controller,
        };
        match (p.kind, reply) {
            (0, Reply::Newspaper(bytes)) => {
                validate_newspaper(bytes)?;
                self.last_news = Some(bytes.clone());
                a.binary(target, "newspaper_state", bytes.clone())
            }
            (1, Reply::Bulletin { last_id, activity }) => {
                self.last_bulletin = Some((*last_id, *activity));
                a.object(
                    target,
                    1,
                    vec![*last_id as i16, (*last_id >> 16) as i16, *activity as i16],
                );
            }
            (
                3,
                Reply::Cooldown {
                    allowed,
                    expires_ticks,
                },
            ) => {
                if !(0..=MAX_DATETIME).contains(expires_ticks) {
                    return Err(Error::InvalidPluginData);
                }
                let local = p.mode <= 1 || (self.community && matches!(p.mode, 5 | 7));
                let account = matches!(p.mode, 1 | 3 | 6 | 7);
                let base = if local {
                    1
                } else if p.mode >= 4 {
                    3
                } else {
                    5
                };
                let code = base + i16::from(account) + if *allowed { 100 } else { 0 };
                let duration = if *allowed {
                    p.duration
                } else {
                    expires_ticks
                        .checked_sub(p.now)
                        .ok_or(Error::InvalidPluginData)?
                };
                a.object(target, code, remaining(duration));
            }
            (4, Reply::Collection(ids)) => {
                if ids.len() > 4096
                    || ids.contains(&0)
                    || ids
                        .iter()
                        .copied()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        != ids.len()
                {
                    return Err(Error::InvalidPluginData);
                }
                self.collection = Some(ids.clone());
            }
            (3, Reply::ProviderDenied) => a.object(target, 7, vec![0]),
            (_, Reply::ProviderDenied) => a.close(Target::All),
            _ => return Err(Error::ProviderReceiptMismatch),
        }
        self.pending.remove(&callback);
        Ok(a)
    }
    pub(super) fn rebind(&mut self, m: Member, _: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        match self.kind {
            0 => {
                a.text(Target::Member(m.seat), "newspaper_show", String::new());
                if let Some(bytes) = &self.last_news {
                    a.binary(Target::Member(m.seat), "newspaper_state", bytes.clone())
                }
            }
            1 => a.text(
                Target::Member(m.seat),
                "bulletin_show",
                m.input.registers[0].to_string(),
            ),
            2 => a.binary(
                Target::Member(m.seat),
                "property_show",
                self.value.to_le_bytes().to_vec(),
            ),
            3 => {}
            4 => a.text(
                Target::Member(m.seat),
                "trunk_fill_UI",
                self.collection_path(),
            ),
            _ => return Err(Error::WrongPlugin),
        }
        Ok(a)
    }
    pub(super) fn leave(&mut self, _: Member, _: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        a.close(Target::Controller);
        Ok(a)
    }
    pub(super) fn shutdown(&mut self, _: &Roster, _: u64) -> Result<Actions, Error> {
        Ok(Actions::default())
    }
    fn valid_pending(&self, p: &Pending) -> bool {
        if p.kind != self.kind || p.seat.is_some_and(|seat| seat as usize >= MAX_MEMBERS) {
            return false;
        }
        let member = p.seat.is_some() && p.avatar != 0;
        match p.kind {
            0 | 4 => member,
            1 => member || p.seat.is_none() && p.avatar == 0,
            3 => {
                member
                    && (0..=7).contains(&p.mode)
                    && (0..=self.now).contains(&p.now)
                    && p.duration % TICKS_PER_SECOND == 0
                    && (i64::from(i16::MIN) * 3661 * TICKS_PER_SECOND
                        ..=i64::from(i16::MAX) * 3661 * TICKS_PER_SECOND)
                        .contains(&p.duration)
                    && p.now
                        .checked_add(p.duration)
                        .is_some_and(|value| (0..=MAX_DATETIME).contains(&value))
            }
            _ => false,
        }
    }
    pub(super) fn validate_context(&self, roster: &Roster, closing: bool) -> bool {
        closing
            || self.pending.values().all(|p| {
                p.seat.is_none_or(|seat| {
                    roster
                        .get(seat as usize)
                        .and_then(|m| *m)
                        .is_some_and(|m| m.avatar_id == p.avatar)
                })
            })
    }
    pub(super) fn validate(&self, roster: &Roster) -> bool {
        self.kind <= 4
            && self.next != 0
            && self.pending.len() <= 4
            && self
                .pending
                .iter()
                .all(|(id, p)| *id > 0 && *id < self.next && self.valid_pending(p))
            && (self.kind != 1 || self.lot != 0)
            && (self.kind != 3 || self.object != 0 && self.lot != 0 && self.guid != 0)
            && self.collection.as_ref().is_none_or(|items| {
                items.len() <= 4096
                    && items.iter().all(|id| *id != 0)
                    && items
                        .iter()
                        .copied()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == items.len()
            })
            && self
                .last_news
                .as_ref()
                .is_none_or(|bytes| validate_newspaper(bytes).is_ok())
            && roster.iter().flatten().count() <= 1
            && self.next_anim.is_none_or(|c| (2..=4).contains(&c))
            && (self.anim || self.next_anim.is_none())
            && (self.kind != 4 || self.category <= 8 && self.value <= 1)
            && (self.kind != 3 || (0..=MAX_DATETIME).contains(&self.now))
    }
    pub(super) fn pending_operations(&self) -> Vec<(u64, Operation)> {
        self.pending
            .iter()
            .map(|(id, p)| {
                let op = match p.kind {
                    0 => Operation::DynamicPayouts,
                    1 => Operation::BulletinState { lot: self.lot },
                    3 => {
                        let local = p.mode <= 1 || (self.community && matches!(p.mode, 5 | 7));
                        Operation::Cooldown {
                            object: self.object,
                            object_guid: self.guid,
                            lot: self.lot,
                            category: if !local && p.mode >= 4 {
                                Some(self.category)
                            } else {
                                None
                            },
                            local,
                            by_account: matches!(p.mode, 1 | 3 | 6 | 7),
                            avatar: p.avatar,
                            now_ticks: p.now,
                            duration_ticks: p.duration,
                        }
                    }
                    4 => Operation::ReadTrunkCollection {
                        kind: self.category,
                        gender: self.value as u8,
                    },
                    _ => unreachable!(),
                };
                (*id, op)
            })
            .collect()
    }
    pub(super) fn save(&self, w: &mut Writer) {
        w.u8(self.kind);
        w.u32(self.object);
        w.u32(self.lot);
        w.u32(self.guid);
        w.u8(self.category);
        w.bool(self.community);
        w.i64(self.now);
        w.u32(self.value);
        w.bool(self.shown);
        w.bool(self.posted);
        w.bool(self.anim);
        w.bool(self.next_anim.is_some());
        if let Some(v) = self.next_anim {
            w.i16(v)
        }
        w.bool(self.collection.is_some());
        if let Some(v) = &self.collection {
            w.u32(v.len() as u32);
            for id in v {
                w.u64(*id)
            }
        }
        w.bool(self.last_news.is_some());
        if let Some(v) = &self.last_news {
            w.bytes(v)
        }
        w.bool(self.last_bulletin.is_some());
        if let Some((id, activity)) = self.last_bulletin {
            w.u32(id);
            w.u32(activity)
        }
        w.u64(self.next);
        w.u32(self.pending.len() as u32);
        for (id, p) in &self.pending {
            w.u64(*id);
            w.bool(p.seat.is_some());
            if let Some(v) = p.seat {
                w.u8(v)
            }
            w.u32(p.avatar);
            w.u8(p.kind);
            w.i16(p.mode);
            w.i64(p.duration);
            w.i64(p.now)
        }
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let kind = r.u8()?;
        let object = r.u32()?;
        let lot = r.u32()?;
        let guid = r.u32()?;
        let category = r.u8()?;
        let community = r.bool()?;
        let now = r.i64()?;
        let value = r.u32()?;
        let shown = r.bool()?;
        let posted = r.bool()?;
        let anim = r.bool()?;
        let next_anim = if r.bool()? { Some(r.i16()?) } else { None };
        let collection = if r.bool()? {
            let n = r.count(4096)?;
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(r.u64()?)
            }
            Some(v)
        } else {
            None
        };
        let last_news = if r.bool()? {
            let b = r.bytes(256 * 1024)?;
            validate_newspaper(&b).map_err(|_| Error::InvalidCheckpoint)?;
            Some(b)
        } else {
            None
        };
        let last_bulletin = if r.bool()? {
            Some((r.u32()?, r.u32()?))
        } else {
            None
        };
        let next = r.u64()?;
        let n = r.count(4)?;
        let mut pending = BTreeMap::new();
        for _ in 0..n {
            let id = r.u64()?;
            let seat = if r.bool()? { Some(r.u8()?) } else { None };
            let p = Pending {
                seat,
                avatar: r.u32()?,
                kind: r.u8()?,
                mode: r.i16()?,
                duration: r.i64()?,
                now: r.i64()?,
            };
            if p.kind != kind
                || p.seat.is_some_and(|s| s as usize >= MAX_MEMBERS)
                || pending.insert(id, p).is_some()
            {
                return Err(Error::InvalidCheckpoint);
            }
        }
        let s = Self {
            kind,
            object,
            lot,
            guid,
            category,
            community,
            now,
            value,
            shown,
            posted,
            anim,
            next_anim,
            collection,
            last_news,
            last_bulletin,
            next,
            pending,
        };
        if !s.validate(&[None; MAX_MEMBERS]) {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(s)
    }
}
fn remaining(ticks: i64) -> Vec<i16> {
    let secs = ticks / TICKS_PER_SECOND;
    vec![
        (secs / 3600) as i16,
        ((secs / 60) % 60) as i16,
        (secs % 60) as i16,
    ]
}
fn validate_newspaper(bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() > 256 * 1024 {
        return Err(Error::InvalidPluginData);
    }
    let mut r = SourceReader::new(bytes);
    let news = r.u32()? as usize;
    if news > 128 {
        return Err(Error::InvalidPluginData);
    }
    for _ in 0..news {
        r.u32()?;
        r.string(1024, 256)?;
        r.string(16384, 4096)?;
        r.i64()?;
        r.i64()?;
    }
    let points = r.u32()? as usize;
    if points > 10000 {
        return Err(Error::InvalidPluginData);
    }
    for _ in 0..points {
        r.take(16)?;
    }
    r.finish()
}

/// Source local cooldown tuple. Native providers use this bounded parser and
/// transactionally compare/write the resulting replacement under their lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CooldownRecord {
    pub avatar: u32,
    pub account: u32,
    pub expiry_ticks: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CooldownDecision {
    pub allowed: bool,
    pub expiry_ticks: i64,
    pub replacement: Vec<CooldownRecord>,
}
pub fn evaluate_local_cooldown(
    bytes: &[u8],
    avatar: u32,
    account: u32,
    by_account: bool,
    now: i64,
    duration: i64,
) -> Result<CooldownDecision, Error> {
    if avatar == 0
        || (by_account && account == 0)
        || bytes.len() > 65536
        || !bytes.len().is_multiple_of(16)
        || !(0..=MAX_DATETIME).contains(&now)
    {
        return Err(Error::InvalidPluginData);
    }
    let expiry = now
        .checked_add(duration)
        .filter(|v| (0..=MAX_DATETIME).contains(v))
        .ok_or(Error::InvalidPluginInput)?;
    let mut r = SourceReader::new(bytes);
    let mut entries = Vec::with_capacity(bytes.len() / 16 + 1);
    while r.at < bytes.len() {
        let v = CooldownRecord {
            avatar: r.u32()?,
            account: r.u32()?,
            expiry_ticks: r.i64()?,
        };
        if v.avatar == 0 || !(0..=MAX_DATETIME).contains(&v.expiry_ticks) {
            return Err(Error::InvalidPluginData);
        }
        entries.push(v)
    }
    let found = entries.iter().position(|v| {
        if by_account {
            v.account == account
        } else {
            v.avatar == avatar
        }
    });
    if let Some(index) = found {
        if now < entries[index].expiry_ticks {
            return Ok(CooldownDecision {
                allowed: false,
                expiry_ticks: entries[index].expiry_ticks,
                replacement: entries,
            });
        }
        entries.remove(index);
    }
    if entries.len() >= 4096 {
        return Err(Error::PersistenceLimit);
    }
    entries.push(CooldownRecord {
        avatar,
        account,
        expiry_ticks: expiry,
    });
    Ok(CooldownDecision {
        allowed: true,
        expiry_ticks: expiry,
        replacement: entries,
    })
}
pub fn encode_local_cooldowns(records: &[CooldownRecord]) -> Result<Vec<u8>, Error> {
    if records.len() > 4096 {
        return Err(Error::PersistenceLimit);
    }
    let mut out = Vec::with_capacity(records.len() * 16);
    for r in records {
        if r.avatar == 0 || !(0..=MAX_DATETIME).contains(&r.expiry_ticks) {
            return Err(Error::InvalidPluginData);
        }
        out.extend(r.avatar.to_le_bytes());
        out.extend(r.account.to_le_bytes());
        out.extend(r.expiry_ticks.to_le_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod private_invariant_tests {
    use super::*;
    #[test]
    fn property_snapshot_cannot_invent_an_unhandled_provider_kind() {
        let mut s = Simple::new(Config::PropertySelect).unwrap();
        s.next = 2;
        s.pending.insert(
            1,
            Pending {
                seat: Some(0),
                avatar: 1,
                kind: 2,
                mode: 0,
                duration: 0,
                now: 0,
            },
        );
        let mut w = Writer::default();
        s.save(&mut w);
        assert!(matches!(
            Simple::restore(&mut Reader::new(&w.0)),
            Err(Error::InvalidCheckpoint)
        ));
    }
    #[test]
    fn cooldown_snapshot_validates_mode_datetime_and_account_identity() {
        let mut s = Simple::new(Config::Cooldown {
            persistent_object: 10,
            object_guid: 20,
            lot: 30,
            category: 7,
            community: false,
            utc_ticks: 100_000_000,
        })
        .unwrap();
        s.next = 2;
        s.pending.insert(
            1,
            Pending {
                seat: Some(0),
                avatar: 1,
                kind: 3,
                mode: 0,
                duration: 10_000_000,
                now: 100_000_000,
            },
        );
        assert!(s.validate(&[None; MAX_MEMBERS]));
        s.pending.get_mut(&1).unwrap().mode = 8;
        assert!(!s.validate(&[None; MAX_MEMBERS]));
        s.pending.get_mut(&1).unwrap().mode = 0;
        s.pending.get_mut(&1).unwrap().now = i64::MAX;
        assert!(!s.validate(&[None; MAX_MEMBERS]));
    }
}
