// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Carducopia source editor/draw state. Source draws uniformly over unique cards;
//! its editable frequency field changes displayed totals, not draw probability.
use super::{types::*, wire::*};
use crate::{
    plugins::{
        ProviderOperation,
        codec::{Reader, Writer},
        common::*,
    },
    *,
};
const CUSTOM: &str = "VMEODGameCompDrawACardCustom";
#[derive(Clone)]
struct Card {
    text: String,
    frequency: u8,
}
#[derive(Clone)]
pub(crate) struct Draw {
    object: u32,
    title: String,
    description: String,
    cards: Vec<Card>,
    current: usize,
    rng: u64,
    draws: u64,
    loaded: bool,
    initialized: bool,
    mode: u8,
    dirty: bool,
    failed: bool,
    revision: u64,
    next: u64,
    load: Option<u64>,
    save: Option<u64>,
    member: Option<(u8, u32)>,
}
impl Draw {
    pub(super) fn new(object: u32, seed: u64) -> Result<Self, Error> {
        if object == 0 {
            return Err(Error::InvalidPluginInput);
        }
        let words = [
            "One",
            "Two",
            "Three",
            "Four",
            "Five",
            "Six",
            "Seven",
            "Eight",
            "Nine",
            "Ten",
            "Eleven",
            "Twelve",
            "Thirteen",
            "Fourteen",
            "Fifteen",
            "Sixteen",
            "Seventeen",
            "Eighteen",
            "Nineteen",
        ];
        Ok(Self {
            object,
            title: "Tsomania Carducopia Card Game".into(),
            description: "Tsomania Carducopia Card Game".into(),
            cards: words
                .iter()
                .map(|s| Card {
                    text: format!("VMEODGameCompDrawACardDefault{s}"),
                    frequency: 1,
                })
                .collect(),
            current: 0,
            rng: seed,
            draws: 0,
            loaded: false,
            initialized: false,
            mode: 4,
            dirty: false,
            failed: false,
            revision: 0,
            next: 1,
            load: None,
            save: None,
            member: None,
        })
    }
    pub(super) fn trusted_object(&self) -> Option<u32> {
        Some(self.object)
    }
    pub(super) fn plugin(&self) -> PluginId {
        PluginId(0x895C1CEB)
    }
    fn id(&mut self) -> Result<u64, Error> {
        let id = self.next;
        self.next = self.next.checked_add(1).ok_or(Error::CounterExhausted)?;
        Ok(id)
    }
    pub(super) fn start(&mut self, _: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        let id = self.id()?;
        self.load = Some(id);
        a.provider(
            id,
            ProviderOperation::Service(Operation::LoadPluginData {
                object: self.object,
                plugin: self.plugin(),
            }),
        );
        Ok(a)
    }
    pub(super) fn join(&mut self, m: Member, roster: &Roster, _: u64) -> Result<Actions, Error> {
        if roster.iter().flatten().count() > 1 || self.member.is_some() {
            return Err(Error::ParticipantLimit);
        }
        let mode = if (0..=2).contains(&m.input.registers[0]) {
            m.input.registers[0] as u8
        } else {
            3
        };
        if mode == 0 && !m.input.owner_authorized {
            return Err(Error::NotAuthorized);
        }
        self.mode = mode;
        self.member = Some((m.seat, m.avatar_id));
        let mut a = Actions::default();
        if self.loaded {
            self.initialize(m.seat, &mut a, true)?
        } else {
            a.binary(Target::Member(m.seat), "DrawCard_Please_Wait", vec![mode]);
        }
        Ok(a)
    }
    fn total(&self) -> u32 {
        self.cards.iter().map(|c| u32::from(c.frequency)).sum()
    }
    fn numbers(&self) -> Vec<u8> {
        strings(&[&self.cards.len().to_string(), &self.total().to_string()])
    }
    fn current_bytes(&self) -> Vec<u8> {
        match self.cards.get(self.current) {
            Some(c) => strings(&[&c.text, &c.frequency.to_string()]),
            None => vec![],
        }
    }
    fn deck(&self) -> Vec<u8> {
        if self.cards.is_empty() {
            return vec![0];
        }
        let mut bytes = vec![];
        for c in &self.cards {
            write_string(&mut bytes, &truncated_utf16(&c.text, 40));
        }
        bytes
    }
    fn initialize(&mut self, seat: u8, a: &mut Actions, draw: bool) -> Result<(), Error> {
        self.initialized = true;
        let target = Target::Member(seat);
        match self.mode {
            0 => {
                a.binary(target, "DrawCard_Update_Deck", self.deck());
                a.binary(target, "DrawCard_Update_Deck_Numbers", self.numbers());
                a.binary(
                    target,
                    "DrawCard_Manage",
                    strings(&[&self.title, &self.description]),
                );
            }
            1 => a.binary(target, "DrawCard_Drawn", self.current_bytes()),
            2 => {
                a.binary(target, "DrawCard_Update_Deck_Numbers", self.numbers());
                a.binary(
                    target,
                    "DrawCard_Info",
                    strings(&[&self.title, &self.description]),
                );
            }
            3 => {
                if draw && !self.cards.is_empty() {
                    self.current = self.random(self.cards.len())?;
                    self.dirty = true;
                }
                a.binary(target, "DrawCard_Drawn", self.current_bytes());
            }
            _ => return Err(Error::InvalidPluginInput),
        }
        Ok(())
    }
    fn random(&mut self, upper: usize) -> Result<usize, Error> {
        let bound = upper as u64;
        if bound == 0 || bound > 300 {
            return Err(Error::InvalidPluginInput);
        }
        let threshold = bound.wrapping_neg() % bound;
        for _ in 0..32 {
            self.draws = self
                .draws
                .checked_add(1)
                .filter(|v| *v != u64::MAX)
                .ok_or(Error::CounterExhausted)?;
            self.rng = self.rng.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = self.rng;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            z ^= z >> 31;
            if z >= threshold {
                return Ok((z % bound) as usize);
            }
        }
        Err(Error::InvalidPluginInput)
    }
    pub(super) fn allows(&self, event: &str, binary: bool) -> bool {
        if binary {
            matches!(
                event,
                "DrawCard_Delete_Card"
                    | "DrawCard_Edit_Frequency"
                    | "DrawCard_Goto_Card"
                    | "DrawCard_Edit_Card"
                    | "DrawCard_Add_Card"
                    | "DrawCard_Edit_Game"
            )
        } else {
            event == "DrawCard_Close"
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
        let t = Target::Member(m.seat);
        if event == "DrawCard_Close" {
            a.close(Target::All);
            return Ok(a);
        }
        if !self.loaded {
            return Err(Error::PluginNotReady);
        }
        if self.save.is_some() {
            return Err(Error::PersistencePending);
        }
        if self.mode != 0 || !m.input.owner_authorized {
            return Err(Error::NotAuthorized);
        }
        match event {
            "DrawCard_Delete_Card" => {
                if bytes.is_empty() {
                    return Err(Error::InvalidMessage);
                }
                if self.cards.is_empty() {
                    a.text(t, "DrawCard_Delete_Fail", String::new())
                } else {
                    self.cards.remove(self.current);
                    self.current = self.current.min(self.cards.len().saturating_sub(1));
                    self.dirty = true;
                    a.binary(t, "DrawCard_Update_Deck", self.deck());
                    if !self.cards.is_empty() {
                        a.binary(t, "DrawCard_Update_Card", self.current_bytes())
                    }
                    a.binary(t, "DrawCard_Update_Deck_Numbers", self.numbers())
                }
            }
            "DrawCard_Edit_Frequency" => {
                if let Some(value) = bytes.first().filter(|v| (1..100).contains(*v))
                    && let Some(c) = self.cards.get_mut(self.current)
                {
                    c.frequency = *value;
                    self.dirty = true;
                }
                a.binary(t, "DrawCard_Update_Deck_Numbers", self.numbers());
            }
            "DrawCard_Goto_Card" => {
                let index = usize::from(*bytes.first().ok_or(Error::InvalidMessage)?);
                if index < self.cards.len() {
                    self.current = index;
                    self.dirty = true;
                }
                a.binary(t, "DrawCard_Update_Card", self.current_bytes());
            }
            "DrawCard_Edit_Card" => {
                let ss = read_strings(bytes, 1, 256)?;
                let text = ss.first().ok_or(Error::InvalidMessage)?;
                let text = if text.is_empty() { CUSTOM } else { text };
                if let Some(c) = self.cards.get_mut(self.current)
                    && c.text != text
                {
                    c.text = text.into();
                    self.dirty = true;
                    a.binary(t, "DrawCard_Update_Deck", self.deck());
                }
            }
            "DrawCard_Add_Card" => {
                let ss = read_strings(bytes, 2, 256)?;
                if ss.len() != 2 {
                    return Err(Error::InvalidMessage);
                }
                if self.cards.len() < 300 {
                    let text = if ss[0].is_empty() { CUSTOM } else { &ss[0] };
                    let frequency = parse_u32(ss[1].as_bytes())
                        .and_then(|v| u8::try_from(v).ok())
                        .unwrap_or(1);
                    self.cards.push(Card {
                        text: text.into(),
                        frequency,
                    });
                    self.dirty = true;
                    a.binary(t, "DrawCard_Update_Deck", self.deck());
                    a.binary(t, "DrawCard_Update_Deck_Numbers", self.numbers());
                }
                a.binary(t, "DrawCard_Update_Card", self.current_bytes());
            }
            "DrawCard_Edit_Game" => {
                let ss = read_strings(bytes, 2, 256)?;
                if ss.len() != 2 || ss[0].encode_utf16().count() > 52 {
                    return Err(Error::InvalidMessage);
                }
                let title = if ss[0].is_empty() { CUSTOM } else { &ss[0] };
                let description = if ss[1].is_empty() { CUSTOM } else { &ss[1] };
                if title != self.title || description != self.description {
                    self.title = title.into();
                    self.description = description.into();
                    self.dirty = true;
                }
            }
            _ => return Err(Error::EventNotAllowed),
        }
        Ok(a)
    }
    fn source_bytes(&self) -> Vec<u8> {
        let mut out = strings(&[&self.title, &self.description]);
        out.push(self.current as u8);
        for c in &self.cards {
            write_string(&mut out, &c.text);
            out.push(c.frequency);
        }
        out
    }
    fn load_source(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > 256 * 1024 {
            return Err(Error::InvalidPluginData);
        }
        let mut r = SourceReader::new(bytes);
        let title = r.string(156, 52)?;
        let description = r.string(768, 256)?;
        let current = usize::from(r.u8()?);
        let mut cards = vec![];
        while r.at < bytes.len() {
            if cards.len() >= 300 {
                return Err(Error::InvalidPluginData);
            }
            cards.push(Card {
                text: r.string(768, 256)?,
                frequency: r.u8()?,
            });
        }
        self.title = title;
        self.description = description;
        self.cards = cards;
        self.current = if current < self.cards.len() {
            current
        } else {
            0
        };
        Ok(())
    }
    fn queue_save(&mut self, a: &mut Actions) -> Result<(), Error> {
        if !self.dirty || !self.loaded || self.save.is_some() {
            return Ok(());
        }
        self.revision
            .checked_add(1)
            .filter(|v| *v < u64::MAX)
            .ok_or(Error::CounterExhausted)?;
        let id = self.id()?;
        self.save = Some(id);
        self.failed = false;
        a.provider(
            id,
            ProviderOperation::Service(Operation::SavePluginData {
                object: self.object,
                plugin: self.plugin(),
                expected_revision: self.revision,
                bytes: self.source_bytes(),
            }),
        );
        Ok(())
    }
    pub(super) fn reply(
        &mut self,
        callback: u64,
        reply: &Reply,
        roster: &Roster,
        _: u64,
    ) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if self.load == Some(callback) {
            match reply {
                Reply::PluginData {
                    exists,
                    revision,
                    bytes,
                } => {
                    if *exists != (*revision != 0)
                        || (!exists && !bytes.is_empty())
                        || *revision == u64::MAX
                    {
                        return Err(Error::InvalidPluginData);
                    }
                    if *exists {
                        self.load_source(bytes)?
                    }
                    self.revision = *revision;
                    self.loaded = true;
                }
                Reply::ProviderDenied => {
                    self.failed = true;
                    a.close(Target::All)
                }
                _ => return Err(Error::ProviderReceiptMismatch),
            }
            self.load = None;
            if self.loaded
                && !self.initialized
                && let Some((seat, avatar)) = self.member
                && roster
                    .get(seat as usize)
                    .and_then(|v| *v)
                    .is_some_and(|m| m.avatar_id == avatar)
            {
                self.initialize(seat, &mut a, true)?
            }
        } else if self.save == Some(callback) {
            match reply {
                Reply::Saved { revision } if self.revision.checked_add(1) == Some(*revision) => {
                    self.revision = *revision;
                    self.dirty = false;
                    self.failed = false;
                    a.object(Target::Controller, 1, vec![])
                }
                Reply::ProviderDenied => self.failed = true,
                _ => return Err(Error::ProviderReceiptMismatch),
            }
            self.save = None;
        } else {
            return Err(Error::ProviderReceiptMismatch);
        }
        Ok(a)
    }
    pub(super) fn rebind(&mut self, m: Member, _: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if self.loaded {
            self.initialize(m.seat, &mut a, false)?
        } else {
            a.binary(
                Target::Member(m.seat),
                "DrawCard_Please_Wait",
                vec![self.mode],
            );
        }
        Ok(a)
    }
    pub(super) fn leave(&mut self, _: Member, _: &Roster, _: u64) -> Result<Actions, Error> {
        self.member = None;
        let mut a = Actions::default();
        self.queue_save(&mut a)?;
        a.close(Target::Controller);
        Ok(a)
    }
    pub(super) fn shutdown(&mut self, _: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        self.queue_save(&mut a)?;
        Ok(a)
    }
    pub(super) fn vm_event(
        &mut self,
        input: &VmInput,
        _: &Roster,
        _: u64,
    ) -> Result<Actions, Error> {
        let mut a = Actions::default();
        match input {
            VmInput::RetryPersistence if self.failed && self.dirty => self.queue_save(&mut a)?,
            VmInput::AbortUncommittedChanges if self.save.is_none() => {
                self.dirty = false;
                self.failed = false;
            }
            _ => return Err(Error::WrongPlugin),
        }
        Ok(a)
    }
    pub(super) fn can_close(&self) -> bool {
        !self.dirty && self.save.is_none()
    }
    pub(super) fn validate(&self, roster: &Roster) -> bool {
        self.object != 0
            && self.next > 0
            && self.cards.len() <= 300
            && self.current < self.cards.len().max(1)
            && self.title.encode_utf16().count() <= 52
            && self.description.encode_utf16().count() <= 256
            && self
                .cards
                .iter()
                .all(|c| c.text.encode_utf16().count() <= 256)
            && self.draws < u64::MAX
            && self.revision < u64::MAX
            && self.mode <= 4
            && self.load.is_none_or(|v| v > 0 && v < self.next)
            && self.save.is_none_or(|v| v > 0 && v < self.next)
            && !(self.save.is_some() && self.load.is_some())
            && self.member.is_none_or(|(seat, avatar)| {
                roster
                    .get(seat as usize)
                    .and_then(|m| *m)
                    .is_some_and(|m| m.avatar_id == avatar)
            })
            && roster.iter().flatten().count() == usize::from(self.member.is_some())
            && self.member.is_none_or(|(seat, _)| {
                self.mode <= 3
                    && (self.mode != 0
                        || roster[seat as usize].is_some_and(|m| m.input.owner_authorized))
            })
            && (!self.initialized || self.loaded)
            && (self.save.is_none() || self.loaded && self.dirty)
            && (!self.loaded || self.load.is_none())
    }
    pub(super) fn pending_operations(&self) -> Vec<(u64, Operation)> {
        let mut v = vec![];
        if let Some(id) = self.load {
            v.push((
                id,
                Operation::LoadPluginData {
                    object: self.object,
                    plugin: self.plugin(),
                },
            ))
        }
        if let Some(id) = self.save {
            v.push((
                id,
                Operation::SavePluginData {
                    object: self.object,
                    plugin: self.plugin(),
                    expected_revision: self.revision,
                    bytes: self.source_bytes(),
                },
            ))
        }
        v
    }
    pub(super) fn save(&self, w: &mut Writer) {
        w.u32(self.object);
        w.string(&self.title);
        w.string(&self.description);
        w.u32(self.cards.len() as u32);
        for c in &self.cards {
            w.string(&c.text);
            w.u8(c.frequency)
        }
        w.u32(self.current as u32);
        w.u64(self.rng);
        w.u64(self.draws);
        w.bool(self.loaded);
        w.bool(self.initialized);
        w.u8(self.mode);
        w.bool(self.dirty);
        w.bool(self.failed);
        w.u64(self.revision);
        w.u64(self.next);
        for v in [self.load, self.save] {
            w.bool(v.is_some());
            if let Some(v) = v {
                w.u64(v)
            }
        }
        w.bool(self.member.is_some());
        if let Some((seat, avatar)) = self.member {
            w.u8(seat);
            w.u32(avatar)
        }
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let object = r.u32()?;
        let title = r.string(156)?;
        let description = r.string(768)?;
        let n = r.count(300)?;
        let mut cards = Vec::with_capacity(n);
        for _ in 0..n {
            cards.push(Card {
                text: r.string(768)?,
                frequency: r.u8()?,
            })
        }
        Ok(Self {
            object,
            title,
            description,
            cards,
            current: r.u32()? as usize,
            rng: r.u64()?,
            draws: r.u64()?,
            loaded: r.bool()?,
            initialized: r.bool()?,
            mode: r.u8()?,
            dirty: r.bool()?,
            failed: r.bool()?,
            revision: r.u64()?,
            next: r.u64()?,
            load: if r.bool()? { Some(r.u64()?) } else { None },
            save: if r.bool()? { Some(r.u64()?) } else { None },
            member: if r.bool()? {
                Some((r.u8()?, r.u32()?))
            } else {
                None
            },
        })
    }
}
