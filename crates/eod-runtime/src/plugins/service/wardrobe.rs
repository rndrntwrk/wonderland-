// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Source rack/customer/owner/dresser state with native atomic inventory effects.
use super::{types::*, wire::*};
use crate::{
    plugins::{
        ProviderOperation,
        codec::{Reader, Writer},
        common::*,
    },
    *,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone)]
struct Pending {
    operation: Operation,
    stage: u8,
    seat: u8,
    avatar: u32,
    outfit_id: u32,
    value: u64,
    put_on: bool,
    update: bool,
    outfit: Option<Outfit>,
    default: Option<(u8, u64)>,
}
#[derive(Clone)]
pub(crate) struct Wardrobe {
    kind: WardrobeKind,
    object: u32,
    rack_type: u8,
    defaults: [u64; 3],
    name: String,
    name_loaded: bool,
    name_shown: bool,
    name_dirty: bool,
    failed: bool,
    revision: u64,
    name_writer: u32,
    next: u64,
    member: Option<(u8, u32, bool)>,
    pending: BTreeMap<u64, Pending>,
    outfits: Option<Vec<Outfit>>,
}
impl Wardrobe {
    pub(super) fn new(
        kind: WardrobeKind,
        object: u32,
        rack_type: u8,
        defaults: [u64; 3],
    ) -> Result<Self, Error> {
        if object == 0 || rack_type > 8 {
            return Err(Error::InvalidPluginInput);
        }
        Ok(Self {
            kind,
            object,
            rack_type,
            defaults,
            name: "Name your rack".into(),
            name_loaded: false,
            name_shown: false,
            name_dirty: false,
            failed: false,
            revision: 0,
            name_writer: 0,
            next: 1,
            member: None,
            pending: BTreeMap::new(),
            outfits: None,
        })
    }
    pub(super) fn trusted_object(&self) -> Option<u32> {
        Some(self.object)
    }
    pub(super) fn plugin(&self) -> PluginId {
        PluginId(match self.kind {
            WardrobeKind::Customer => 0xCB492685,
            WardrobeKind::Owner => 0x2B58020B,
            WardrobeKind::Dresser => 0x8B300068,
        })
    }
    fn queue(&mut self, a: &mut Actions, p: Pending) -> Result<(), Error> {
        if self.pending.len() >= 8 {
            return Err(Error::PersistencePending);
        }
        let id = self.next;
        self.next = self.next.checked_add(1).ok_or(Error::CounterExhausted)?;
        a.provider(id, ProviderOperation::Service(p.operation.clone()));
        self.pending.insert(id, p);
        Ok(())
    }
    fn pending(stage: u8, seat: u8, avatar: u32, operation: Operation) -> Pending {
        Pending {
            operation,
            stage,
            seat,
            avatar,
            outfit_id: 0,
            value: 0,
            put_on: false,
            update: false,
            outfit: None,
            default: None,
        }
    }
    fn list(&mut self, a: &mut Actions, seat: u8, avatar: u32, update: bool) -> Result<(), Error> {
        let owner = if self.kind == WardrobeKind::Dresser {
            OutfitOwner::Avatar(avatar)
        } else {
            OutfitOwner::Object(self.object)
        };
        let mut p = Self::pending(1, seat, avatar, Operation::ListOutfits { owner });
        p.update = update;
        self.queue(a, p)
    }
    pub(super) fn start(&mut self, _: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        let p = Self::pending(
            0,
            0,
            0,
            Operation::LoadPluginData {
                object: self.object,
                plugin: self.plugin(),
            },
        );
        self.queue(&mut a, p)?;
        Ok(a)
    }
    pub(super) fn join(&mut self, m: Member, roster: &Roster, _: u64) -> Result<Actions, Error> {
        if roster.iter().flatten().count() > 1 || self.member.is_some() {
            return Err(Error::ParticipantLimit);
        }
        if self.kind == WardrobeKind::Owner && !m.input.owner_authorized {
            return Err(Error::NotAuthorized);
        }
        self.member = Some((m.seat, m.avatar_id, m.input.owner_authorized));
        let mut a = Actions::default();
        self.show(m.seat, &mut a);
        self.list(
            &mut a,
            m.seat,
            m.avatar_id,
            m.input.owner_authorized && self.kind != WardrobeKind::Dresser,
        )?;
        Ok(a)
    }
    fn show(&self, seat: u8, a: &mut Actions) {
        if self.kind == WardrobeKind::Dresser {
            a.text(Target::Member(seat), "dresser_show", String::new())
        } else {
            a.text(
                Target::Member(seat),
                "rack_show",
                self.rack_type.to_string(),
            )
        }
    }
    pub(super) fn allows(&self, event: &str, binary: bool) -> bool {
        if binary {
            return false;
        }
        event == "close"
            || match self.kind {
                WardrobeKind::Customer => matches!(event, "rack_try_outfit_on" | "rack_purchase"),
                WardrobeKind::Owner => matches!(
                    event,
                    "rackowner_update_name"
                        | "rackowner_stock"
                        | "rackowner_delete"
                        | "rackowner_update_price"
                ),
                WardrobeKind::Dresser => matches!(
                    event,
                    "dresser_change_outfit" | "dresser_set_default" | "dresser_delete_outfit"
                ),
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
        if event == "close" {
            a.close(Target::All);
            return Ok(a);
        }
        if self.pending.values().any(|p| p.stage != 0) {
            return Err(Error::PersistencePending);
        }
        let body = std::str::from_utf8(bytes).map_err(|_| Error::InvalidMessage)?;
        if self.kind == WardrobeKind::Owner && !m.input.owner_authorized {
            return Err(Error::NotAuthorized);
        }
        match event {
            "rackowner_update_name" => {
                if !self.name_loaded {
                    return Err(Error::PluginNotReady);
                }
                let n = body.encode_utf16().count();
                if (1..=32).contains(&n) {
                    self.name = body.to_string();
                    self.name_dirty = true;
                    self.name_writer = m.avatar_id;
                }
            }
            "rack_try_outfit_on" | "dresser_change_outfit" | "dresser_delete_outfit" => {
                if let Some(id) = parse_u32(bytes) {
                    let stage = match event {
                        "rack_try_outfit_on" => 2,
                        "dresser_change_outfit" => 10,
                        _ => 12,
                    };
                    let owner = if self.kind == WardrobeKind::Dresser {
                        OutfitOwner::Avatar(m.avatar_id)
                    } else {
                        OutfitOwner::Object(self.object)
                    };
                    let mut p =
                        Self::pending(stage, m.seat, m.avatar_id, Operation::ListOutfits { owner });
                    p.outfit_id = id;
                    self.queue(&mut a, p)?
                }
            }
            "rack_purchase" => {
                let ss: Vec<_> = body.split(',').collect();
                if ss.len() == 2
                    && let (Some(id), Some(put_on)) =
                        (parse_u32(ss[0].as_bytes()), parse_bool(ss[1]))
                {
                    let mut p = Self::pending(
                        3,
                        m.seat,
                        m.avatar_id,
                        Operation::ListOutfits {
                            owner: OutfitOwner::Object(self.object),
                        },
                    );
                    p.outfit_id = id;
                    p.put_on = put_on;
                    self.queue(&mut a, p)?
                }
            }
            "rackowner_stock" => {
                if let Some(asset) = parse_u64(bytes)
                    && self.outfits.as_ref().is_none_or(|v| v.len() < 20)
                {
                    let mut p = Self::pending(
                        6,
                        m.seat,
                        m.avatar_id,
                        Operation::ReadCatalog {
                            rack_type: self.rack_type,
                        },
                    );
                    p.value = asset;
                    self.queue(&mut a, p)?
                }
            }
            "rackowner_delete" => {
                if let Some(id) = parse_u32(bytes) {
                    self.queue(
                        &mut a,
                        Self::pending(
                            8,
                            m.seat,
                            m.avatar_id,
                            Operation::DeleteOutfit {
                                owner: OutfitOwner::Object(self.object),
                                actor: m.avatar_id,
                                outfit: id,
                                keep_one_in_category: false,
                            },
                        ),
                    )?
                }
            }
            "rackowner_update_price" => {
                let ss: Vec<_> = body.split(',').collect();
                if ss.len() == 2
                    && let (Some(id), Some(price)) = (
                        parse_u32(ss[0].as_bytes()),
                        parse_i32(ss[1].as_bytes()).filter(|v| (1..=999999).contains(v)),
                    )
                {
                    self.queue(
                        &mut a,
                        Self::pending(
                            9,
                            m.seat,
                            m.avatar_id,
                            Operation::UpdateOutfitPrice {
                                object: self.object,
                                avatar: m.avatar_id,
                                outfit: id,
                                price,
                            },
                        ),
                    )?
                }
            }
            "dresser_set_default" => {
                let ss: Vec<_> = body.split(',').collect();
                if ss.len() == 2
                    && let (Some(category), Some(id)) = (
                        parse_i32(ss[0].as_bytes()).filter(|v| matches!(v, 0 | 2 | 5)),
                        parse_u32(ss[1].as_bytes()),
                    )
                {
                    let mut p = Self::pending(
                        11,
                        m.seat,
                        m.avatar_id,
                        Operation::ListOutfits {
                            owner: OutfitOwner::Avatar(m.avatar_id),
                        },
                    );
                    p.value = category as u64;
                    p.outfit_id = id;
                    self.queue(&mut a, p)?
                }
            }
            _ => return Err(Error::EventNotAllowed),
        }
        Ok(a)
    }
    fn set_outfit(
        &mut self,
        a: &mut Actions,
        p: &Pending,
        outfit: &Outfit,
        try_on: bool,
        dresser: bool,
    ) {
        let (scope, event_arg) = if dresser {
            match outfit.outfit_type {
                0 => (OutfitScope::DynamicDaywear, 100),
                5 => (OutfitScope::DynamicSleepwear, 101),
                2 => (OutfitScope::DynamicSwimwear, 102),
                8 => (OutfitScope::DecorationHead, 3),
                9 => (OutfitScope::DecorationBack, 4),
                10 => (OutfitScope::DecorationShoes, 5),
                11 => (OutfitScope::DecorationTail, 6),
                _ => return,
            }
        } else {
            (
                if try_on && matches!(self.rack_type, 0..=3 | 8) {
                    OutfitScope::DynamicCostume
                } else {
                    scope(suit_type(self.rack_type))
                },
                i16::from(self.rack_type),
            )
        };
        a.command(NativeCommand::SetOutfit {
            avatar_id: p.avatar,
            scope,
            outfit: outfit.asset_id,
        });
        a.object(
            Target::Member(p.seat),
            if dresser || try_on { 1 } else { 3 },
            vec![event_arg],
        );
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
        let active = roster
            .get(p.seat as usize)
            .and_then(|v| *v)
            .is_some_and(|m| m.avatar_id == p.avatar);
        let target = Target::Member(p.seat);
        if matches!(reply, Reply::ProviderDenied) {
            self.pending.remove(&callback);
            if p.stage == 14 {
                self.failed = true
            } else if p.stage == 0 || active {
                a.close(Target::All)
            }
            return Ok(a);
        }
        match (p.stage, reply) {
            (
                0,
                Reply::PluginData {
                    exists,
                    revision,
                    bytes,
                },
            ) => {
                if *exists != (*revision != 0)
                    || *revision == u64::MAX
                    || (!exists && !bytes.is_empty())
                {
                    return Err(Error::InvalidPluginData);
                }
                if *exists {
                    let mut r = SourceReader::new(bytes);
                    self.name = r.string(128, 32)?;
                    r.finish()?
                }
                self.revision = *revision;
                self.name_loaded = true;
            }
            (1 | 2 | 3 | 4 | 10 | 11 | 12, Reply::Outfits(outfits)) => {
                let Operation::ListOutfits { owner } = p.operation else {
                    return Err(Error::InvalidCheckpoint);
                };
                if outfits.len() > 4096
                    || outfits.iter().any(|o| !o.valid() || o.owner != owner)
                    || outfits
                        .iter()
                        .map(|o| o.outfit_id)
                        .collect::<BTreeSet<_>>()
                        .len()
                        != outfits.len()
                {
                    return Err(Error::InvalidPluginData);
                }
                if p.stage == 1 {
                    if active {
                        self.outfits = Some(outfits.clone());
                        a.binary(target, "set_outfits", outfit_wire(outfits));
                        if p.update {
                            a.object(target, 8, vec![outfits.len() as i16]);
                        }
                    }
                } else if p.stage == 4 {
                    let outfit = p.outfit.as_ref().ok_or(Error::InvalidCheckpoint)?;
                    if active {
                        if outfits.iter().any(|v| v.asset_id == outfit.asset_id) {
                            a.binary(target, "rack_buy_error", vec![0])
                        } else if outfits
                            .iter()
                            .filter(|v| v.outfit_type == outfit.outfit_type)
                            .count()
                            >= 5
                        {
                            a.binary(target, "rack_buy_error", vec![1])
                        } else {
                            let mut next = p.clone();
                            next.stage = 5;
                            next.operation = Operation::PurchaseOutfit {
                                object: self.object,
                                avatar: p.avatar,
                                outfit: outfit.outfit_id,
                                asset: outfit.asset_id,
                                price: outfit.sale_price,
                            };
                            self.queue(&mut a, next)?
                        }
                    }
                } else if let Some(outfit) = outfits.iter().find(|o| o.outfit_id == p.outfit_id)
                    && active
                {
                    match p.stage {
                        2 => self.set_outfit(&mut a, &p, outfit, true, false),
                        3 => {
                            let mut next = p.clone();
                            next.stage = 4;
                            next.outfit = Some(outfit.clone());
                            next.operation = Operation::ListOutfits {
                                owner: OutfitOwner::Avatar(p.avatar),
                            };
                            self.queue(&mut a, next)?
                        }
                        10 => self.set_outfit(&mut a, &p, outfit, false, true),
                        11 if u64::from(outfit.outfit_type) == p.value => {
                            let index = default_index(outfit.outfit_type)
                                .ok_or(Error::InvalidPluginData)?;
                            self.defaults[index] = outfit.asset_id;
                            a.command(NativeCommand::SetOutfit {
                                avatar_id: p.avatar,
                                scope: scope(outfit.outfit_type),
                                outfit: outfit.asset_id,
                            });
                            a.text(target, "dresser_refresh_default", String::new());
                        }
                        12 => {
                            let decoration = outfit.outfit_type >= 8;
                            let others: Vec<_> = outfits
                                .iter()
                                .filter(|o| {
                                    o.outfit_type == outfit.outfit_type
                                        && o.outfit_id != outfit.outfit_id
                                })
                                .collect();
                            if decoration || !others.is_empty() {
                                let mut next = p.clone();
                                next.stage = 13;
                                next.outfit = Some(outfit.clone());
                                next.operation = Operation::DeleteOutfit {
                                    owner: OutfitOwner::Avatar(p.avatar),
                                    actor: p.avatar,
                                    outfit: outfit.outfit_id,
                                    keep_one_in_category: !decoration,
                                };
                                if default_index(outfit.outfit_type).is_some() {
                                    // Freeze the source's first alternate for every
                                    // clothing delete. A trusted default can change
                                    // while the durable request is in flight.
                                    next.default = Some((outfit.outfit_type, others[0].asset_id));
                                }
                                self.queue(&mut a, next)?
                            }
                        }
                        _ => {}
                    }
                }
            }
            (6, Reply::Catalog(catalog)) => {
                if catalog.len() > 4096
                    || catalog.iter().any(|o| {
                        o.asset_id == 0 || o.price < 0 || o.outfit_type != suit_type(self.rack_type)
                    })
                    || catalog
                        .iter()
                        .map(|o| o.asset_id)
                        .collect::<BTreeSet<_>>()
                        .len()
                        != catalog.len()
                {
                    return Err(Error::InvalidPluginData);
                }
                if active && let Some(o) = catalog.iter().find(|o| o.asset_id == p.value) {
                    let mut next = p.clone();
                    next.stage = 7;
                    next.operation = Operation::StockOutfit {
                        object: self.object,
                        avatar: p.avatar,
                        asset: o.asset_id,
                        price: o.price,
                        outfit_type: o.outfit_type,
                    };
                    self.queue(&mut a, next)?
                }
            }
            (5 | 7 | 8 | 9 | 13, Reply::Mutation { success }) => {
                if p.stage == 5 {
                    if *success {
                        if active && p.put_on {
                            self.set_outfit(
                                &mut a,
                                &p,
                                p.outfit.as_ref().ok_or(Error::InvalidCheckpoint)?,
                                false,
                                false,
                            )
                        }
                        if active {
                            self.list(&mut a, p.seat, p.avatar, true)?
                        }
                    } else if active {
                        a.binary(target, "rack_buy_error", vec![2]);
                    }
                } else if p.stage == 7 {
                    if active {
                        a.object(target, 4, vec![0]);
                        if *success {
                            self.list(&mut a, p.seat, p.avatar, true)?
                        }
                    }
                } else if p.stage == 13 {
                    if *success && let Some((category, asset)) = p.default {
                        let expected = p.outfit.as_ref().ok_or(Error::InvalidCheckpoint)?.asset_id;
                        let index = default_index(category).ok_or(Error::InvalidCheckpoint)?;
                        if self.defaults[index] == expected {
                            self.defaults[index] = asset;
                        }
                        a.command(NativeCommand::SetOutfitIfCurrent {
                            avatar_id: p.avatar,
                            scope: scope(category),
                            expected,
                            outfit: asset,
                        });
                    }
                    if active {
                        self.list(&mut a, p.seat, p.avatar, false)?
                    }
                } else if active {
                    self.list(&mut a, p.seat, p.avatar, p.stage == 8)?
                }
            }
            (14, Reply::Saved { revision }) if self.revision.checked_add(1) == Some(*revision) => {
                self.revision = *revision;
                self.name_dirty = false;
                self.failed = false;
            }
            _ => return Err(Error::ProviderReceiptMismatch),
        }
        self.pending.remove(&callback);
        Ok(a)
    }
    pub(super) fn tick(&mut self, _: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        if self.name_loaded
            && !self.name_shown
            && self.kind != WardrobeKind::Dresser
            && let Some((seat, _, _)) = self.member
        {
            a.text(
                Target::Member(seat),
                "rack_initialize_name",
                self.name.clone(),
            );
            self.name_shown = true;
        }
        Ok(a)
    }
    pub(super) fn rebind(&mut self, m: Member, _: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        self.show(m.seat, &mut a);
        if let Some(outfits) = &self.outfits {
            a.binary(Target::Member(m.seat), "set_outfits", outfit_wire(outfits));
        }
        if self.kind != WardrobeKind::Dresser && self.name_loaded {
            a.text(
                Target::Member(m.seat),
                "rack_initialize_name",
                self.name.clone(),
            );
        }
        Ok(a)
    }
    fn queue_name(&mut self, a: &mut Actions, avatar: u32) -> Result<(), Error> {
        if self.name_dirty && !self.pending.values().any(|p| p.stage == 14) {
            if !self.name_loaded {
                return Err(Error::PluginNotReady);
            }
            self.revision
                .checked_add(1)
                .filter(|v| *v < u64::MAX)
                .ok_or(Error::CounterExhausted)?;
            let p = Self::pending(
                14,
                0,
                avatar,
                Operation::SaveRackName {
                    object: self.object,
                    avatar,
                    expected_revision: self.revision,
                    name: self.name.clone(),
                },
            );
            self.failed = false;
            self.queue(a, p)?
        }
        Ok(())
    }
    pub(super) fn leave(&mut self, m: Member, _: &Roster, _: u64) -> Result<Actions, Error> {
        let mut a = Actions::default();
        self.queue_name(&mut a, m.avatar_id)?;
        if self.kind == WardrobeKind::Customer {
            a.object(Target::Member(m.seat), 10, vec![i16::from(self.rack_type)]);
        } else if self.kind == WardrobeKind::Dresser {
            a.object(Target::Member(m.seat), 2, vec![]);
        }
        self.member = None;
        self.outfits = None;
        a.close(Target::Controller);
        Ok(a)
    }
    pub(super) fn shutdown(&mut self, _: &Roster, _: u64) -> Result<Actions, Error> {
        Ok(Actions::default())
    }
    pub(super) fn vm_event(
        &mut self,
        input: &VmInput,
        _: &Roster,
        _: u64,
    ) -> Result<Actions, Error> {
        let mut a = Actions::default();
        match input {
            VmInput::ObserveDefaultOutfits {
                avatar_id,
                defaults,
            } if self.kind == WardrobeKind::Dresser
                && *avatar_id != 0
                && (self
                    .member
                    .is_some_and(|(_, avatar, _)| avatar == *avatar_id)
                    || self.reserved_avatars().contains(avatar_id)) =>
            {
                self.defaults = *defaults;
            }
            VmInput::RetryPersistence if self.failed && self.name_dirty => {
                let avatar = self.name_writer;
                if avatar == 0 {
                    return Err(Error::ReconciliationRequired);
                }
                self.queue_name(&mut a, avatar)?
            }
            VmInput::AbortUncommittedChanges if !self.pending.values().any(|p| p.stage == 14) => {
                self.name_dirty = false;
                self.failed = false;
            }
            _ => return Err(Error::WrongPlugin),
        }
        Ok(a)
    }
    pub(super) fn can_close(&self) -> bool {
        !self.name_dirty
    }
    pub(super) fn reserved_avatars(&self) -> Vec<u32> {
        self.pending
            .values()
            .filter(|p| p.stage == 13 && p.default.is_some())
            .map(|p| p.avatar)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub(super) fn pending_operations(&self) -> Vec<(u64, Operation)> {
        self.pending
            .iter()
            .map(|(id, p)| (*id, p.operation.clone()))
            .collect()
    }
    fn valid_pending(&self, p: &Pending) -> bool {
        if p.seat as usize >= MAX_MEMBERS {
            return false;
        }
        let user = p.avatar != 0;
        let owned = |owner: OutfitOwner| match self.kind {
            WardrobeKind::Dresser => owner == OutfitOwner::Avatar(p.avatar),
            _ => owner == OutfitOwner::Object(self.object),
        };
        let outfit = p.outfit.as_ref();
        let valid_rack_outfit = outfit.is_some_and(|o| {
            o.valid() && o.owner == OutfitOwner::Object(self.object) && o.outfit_id == p.outfit_id
        });
        match (&p.operation, p.stage) {
            (Operation::LoadPluginData { object, plugin }, 0) => {
                *object == self.object && *plugin == self.plugin() && p.avatar == 0 && p.seat == 0
            }
            (Operation::ListOutfits { owner }, 1) => user && owned(*owner),
            (Operation::ListOutfits { owner }, 2 | 3) => {
                user && self.kind == WardrobeKind::Customer
                    && *owner == OutfitOwner::Object(self.object)
            }
            (Operation::ListOutfits { owner }, 4) => {
                user && self.kind == WardrobeKind::Customer
                    && *owner == OutfitOwner::Avatar(p.avatar)
                    && valid_rack_outfit
            }
            (
                Operation::PurchaseOutfit {
                    object,
                    avatar,
                    outfit: id,
                    asset,
                    price,
                },
                5,
            ) => {
                user && self.kind == WardrobeKind::Customer
                    && *object == self.object
                    && *avatar == p.avatar
                    && valid_rack_outfit
                    && outfit.is_some_and(|o| {
                        o.outfit_id == *id && o.asset_id == *asset && o.sale_price == *price
                    })
            }
            (Operation::ReadCatalog { rack_type }, 6) => {
                user && self.kind == WardrobeKind::Owner && *rack_type == self.rack_type
            }
            (
                Operation::StockOutfit {
                    object,
                    avatar,
                    asset,
                    price,
                    outfit_type,
                },
                7,
            ) => {
                user && self.kind == WardrobeKind::Owner
                    && *object == self.object
                    && *avatar == p.avatar
                    && *asset != 0
                    && *asset == p.value
                    && *price >= 0
                    && *outfit_type == suit_type(self.rack_type)
            }
            (
                Operation::DeleteOutfit {
                    owner,
                    actor,
                    keep_one_in_category,
                    ..
                },
                8,
            ) => {
                user && self.kind == WardrobeKind::Owner
                    && *owner == OutfitOwner::Object(self.object)
                    && *actor == p.avatar
                    && !keep_one_in_category
            }
            (
                Operation::UpdateOutfitPrice {
                    object,
                    avatar,
                    price,
                    ..
                },
                9,
            ) => {
                user && self.kind == WardrobeKind::Owner
                    && *object == self.object
                    && *avatar == p.avatar
                    && (1..=999999).contains(price)
            }
            (Operation::ListOutfits { owner }, 10..=12) => {
                user && self.kind == WardrobeKind::Dresser
                    && *owner == OutfitOwner::Avatar(p.avatar)
                    && (p.stage != 11 || matches!(p.value, 0 | 2 | 5))
            }
            (
                Operation::DeleteOutfit {
                    owner,
                    actor,
                    outfit: id,
                    keep_one_in_category,
                },
                13,
            ) => {
                user && self.kind == WardrobeKind::Dresser
                    && *owner == OutfitOwner::Avatar(p.avatar)
                    && *actor == p.avatar
                    && outfit.is_some_and(|o| {
                        o.valid()
                            && o.owner == *owner
                            && o.outfit_id == *id
                            && *id == p.outfit_id
                            && *keep_one_in_category == (o.outfit_type < 8)
                            && p.default.is_some() == default_index(o.outfit_type).is_some()
                            && p.default.is_none_or(|(category, asset)| {
                                category == o.outfit_type
                                    && default_index(category).is_some()
                                    && asset != 0
                            })
                    })
            }
            (
                Operation::SaveRackName {
                    object,
                    avatar,
                    expected_revision,
                    name,
                },
                14,
            ) => {
                self.kind == WardrobeKind::Owner
                    && self.name_dirty
                    && self.name_loaded
                    && user
                    && p.avatar == self.name_writer
                    && *object == self.object
                    && *avatar == self.name_writer
                    && *expected_revision == self.revision
                    && *name == self.name
            }
            _ => false,
        }
    }
    pub(super) fn validate_context(&self, roster: &Roster, closing: bool) -> bool {
        closing
            || self.pending.values().all(|p| {
                p.stage == 0
                    || roster
                        .get(p.seat as usize)
                        .and_then(|m| *m)
                        .is_some_and(|m| m.avatar_id == p.avatar)
            })
    }
    pub(super) fn validate(&self, roster: &Roster) -> bool {
        self.object != 0
            && self.rack_type <= 8
            && self.next != 0
            && self.revision < u64::MAX
            && (!self.name_dirty || self.kind == WardrobeKind::Owner && self.name_writer != 0)
            && self.name.encode_utf16().count() <= 32
            && self.pending.len() <= 8
            && self
                .pending
                .iter()
                .all(|(id, p)| *id > 0 && *id < self.next && self.valid_pending(p))
            && self.member.is_none_or(|(seat, avatar, _)| {
                roster.get(seat as usize).and_then(|m| *m).is_some_and(|m| {
                    m.avatar_id == avatar
                        && m.input.owner_authorized == self.member.unwrap().2
                        && (self.kind != WardrobeKind::Owner || m.input.owner_authorized)
                })
            })
            && roster.iter().flatten().count() == usize::from(self.member.is_some())
            && self.outfits.as_ref().is_none_or(|outfits| {
                outfits.len() <= 4096
                    && outfits.iter().all(|o| {
                        o.valid()
                            && match (self.kind, o.owner) {
                                (WardrobeKind::Dresser, OutfitOwner::Avatar(avatar)) => {
                                    self.member.is_some_and(|(_, member, _)| member == avatar)
                                }
                                (WardrobeKind::Dresser, _) => false,
                                (_, OutfitOwner::Object(object)) => object == self.object,
                                _ => false,
                            }
                    })
                    && outfits
                        .iter()
                        .map(|o| o.outfit_id)
                        .collect::<BTreeSet<_>>()
                        .len()
                        == outfits.len()
            })
    }
    pub(super) fn save(&self, w: &mut Writer) {
        w.u8(match self.kind {
            WardrobeKind::Customer => 0,
            WardrobeKind::Owner => 1,
            WardrobeKind::Dresser => 2,
        });
        w.u32(self.object);
        w.u8(self.rack_type);
        for d in self.defaults {
            w.u64(d)
        }
        w.string(&self.name);
        for b in [
            self.name_loaded,
            self.name_shown,
            self.name_dirty,
            self.failed,
        ] {
            w.bool(b)
        }
        w.u64(self.revision);
        w.u32(self.name_writer);
        w.u64(self.next);
        w.bool(self.member.is_some());
        if let Some((seat, avatar, owner)) = self.member {
            w.u8(seat);
            w.u32(avatar);
            w.bool(owner)
        }
        w.bool(self.outfits.is_some());
        if let Some(outfits) = &self.outfits {
            w.u32(outfits.len() as u32);
            for o in outfits {
                o.save(w)
            }
        }
        w.u32(self.pending.len() as u32);
        for (id, p) in &self.pending {
            w.u64(*id);
            p.operation.save(w);
            w.u8(p.stage);
            w.u8(p.seat);
            w.u32(p.avatar);
            w.u32(p.outfit_id);
            w.u64(p.value);
            w.bool(p.put_on);
            w.bool(p.update);
            w.bool(p.outfit.is_some());
            if let Some(o) = &p.outfit {
                o.save(w)
            }
            w.bool(p.default.is_some());
            if let Some((category, asset)) = p.default {
                w.u8(category);
                w.u64(asset)
            }
        }
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let kind = match r.u8()? {
            0 => WardrobeKind::Customer,
            1 => WardrobeKind::Owner,
            2 => WardrobeKind::Dresser,
            _ => return Err(Error::InvalidCheckpoint),
        };
        let object = r.u32()?;
        let rack_type = r.u8()?;
        let defaults = [r.u64()?, r.u64()?, r.u64()?];
        let name = r.string(128)?;
        let name_loaded = r.bool()?;
        let name_shown = r.bool()?;
        let name_dirty = r.bool()?;
        let failed = r.bool()?;
        let revision = r.u64()?;
        let name_writer = r.u32()?;
        let next = r.u64()?;
        let member = if r.bool()? {
            Some((r.u8()?, r.u32()?, r.bool()?))
        } else {
            None
        };
        let outfits = if r.bool()? {
            let n = r.count(4096)?;
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(Outfit::restore(r)?)
            }
            Some(v)
        } else {
            None
        };
        let n = r.count(8)?;
        let mut pending = BTreeMap::new();
        for _ in 0..n {
            let id = r.u64()?;
            let p = Pending {
                operation: Operation::restore(r)?,
                stage: r.u8()?,
                seat: r.u8()?,
                avatar: r.u32()?,
                outfit_id: r.u32()?,
                value: r.u64()?,
                put_on: r.bool()?,
                update: r.bool()?,
                outfit: if r.bool()? {
                    Some(Outfit::restore(r)?)
                } else {
                    None
                },
                default: if r.bool()? {
                    Some((r.u8()?, r.u64()?))
                } else {
                    None
                },
            };
            if pending.insert(id, p).is_some() {
                return Err(Error::InvalidCheckpoint);
            }
        }
        Ok(Self {
            kind,
            object,
            rack_type,
            defaults,
            name,
            name_loaded,
            name_shown,
            name_dirty,
            failed,
            revision,
            name_writer,
            next,
            member,
            pending,
            outfits,
        })
    }
}
fn parse_bool(s: &str) -> Option<bool> {
    if s.trim().eq_ignore_ascii_case("true") {
        Some(true)
    } else if s.trim().eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}
fn suit_type(rack: u8) -> u8 {
    match rack {
        0 | 1 | 8 => 0,
        2 => 2,
        3 => 5,
        4 => 8,
        5 => 9,
        6 => 10,
        7 => 11,
        _ => 0,
    }
}
fn default_index(category: u8) -> Option<usize> {
    match category {
        0 => Some(0),
        5 => Some(1),
        2 => Some(2),
        _ => None,
    }
}
fn scope(category: u8) -> OutfitScope {
    match category {
        0 => OutfitScope::DefaultDaywear,
        2 => OutfitScope::DefaultSwimwear,
        5 => OutfitScope::DefaultSleepwear,
        8 => OutfitScope::DecorationHead,
        9 => OutfitScope::DecorationBack,
        10 => OutfitScope::DecorationShoes,
        11 => OutfitScope::DecorationTail,
        _ => OutfitScope::DynamicCostume,
    }
}
fn outfit_wire(outfits: &[Outfit]) -> Vec<u8> {
    let mut out = (outfits.len() as i32).to_be_bytes().to_vec();
    for o in outfits {
        out.extend(o.outfit_id.to_be_bytes());
        out.extend(o.asset_id.to_be_bytes());
        out.extend(o.sale_price.to_be_bytes());
        out.extend(o.purchase_price.to_be_bytes());
        let (kind, id) = match o.owner {
            OutfitOwner::Avatar(id) => (1u16, id),
            OutfitOwner::Object(id) => (2u16, id),
        };
        out.extend(kind.to_be_bytes());
        out.extend(id.to_be_bytes());
        out.push(o.outfit_type);
        out.extend(u16::from(o.source).to_be_bytes());
    }
    out
}

#[cfg(test)]
mod private_invariant_tests {
    use super::*;
    #[test]
    fn wardrobe_stage_object_and_retained_writer_must_match_operation() {
        let mut s = Wardrobe::new(WardrobeKind::Owner, 10, 0, [0; 3]).unwrap();
        s.name_loaded = true;
        s.name_dirty = true;
        s.name_writer = 1;
        s.next = 2;
        s.pending.insert(
            1,
            Wardrobe::pending(
                14,
                0,
                1,
                Operation::SaveRackName {
                    object: 10,
                    avatar: 1,
                    expected_revision: 0,
                    name: s.name.clone(),
                },
            ),
        );
        assert!(s.validate(&[None; MAX_MEMBERS]));
        if let Operation::SaveRackName { avatar, .. } =
            &mut s.pending.get_mut(&1).unwrap().operation
        {
            *avatar = 2;
        }
        assert!(!s.validate(&[None; MAX_MEMBERS]));
        s.pending.get_mut(&1).unwrap().operation = Operation::DynamicPayouts;
        assert!(!s.validate(&[None; MAX_MEMBERS]));
    }
}
