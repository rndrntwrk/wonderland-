// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
use super::super::codec::{Reader, Writer};
use crate::{Error, PluginId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WardrobeKind {
    Customer,
    Owner,
    Dresser,
}
#[derive(Clone)]
pub enum Config {
    Newspaper,
    Bulletin {
        lot: u32,
    },
    PropertySelect,
    Cooldown {
        persistent_object: u32,
        object_guid: u32,
        lot: u32,
        category: u8,
        community: bool,
        utc_ticks: i64,
    },
    Wardrobe {
        kind: WardrobeKind,
        object: u32,
        rack_type: u8,
        defaults: [u64; 3],
    },
    Trunk {
        kind: u8,
    },
    DrawCard {
        object: u32,
        seed: u64,
    },
    SecureTrade {
        untradable_guids: Vec<u32>,
    },
}
#[derive(Clone, Debug)]
pub enum VmInput {
    BulletinAnimationFinished {
        seat: u8,
    },
    /// Trusted current UTC DateTime ticks, independent of the simulation tick rate.
    SetUtcTicks(i64),
    /// Authoritative VM observation for this wardrobe's current or retained
    /// avatar, ordered before its next provider continuation at the VM barrier.
    ObserveDefaultOutfits {
        avatar_id: u32,
        defaults: [u64; 3],
    },
    RetryPersistence,
    /// Explicit native abort of unsaved draft text/cards after a provider refusal.
    AbortUncommittedChanges,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutfitOwner {
    Avatar(u32),
    Object(u32),
}
#[derive(Clone, PartialEq, Eq)]
pub struct Outfit {
    pub outfit_id: u32,
    pub asset_id: u64,
    pub sale_price: i32,
    pub purchase_price: i32,
    pub owner: OutfitOwner,
    pub outfit_type: u8,
    pub source: u8,
}
impl Outfit {
    pub(super) fn valid(&self) -> bool {
        self.outfit_id != 0
            && self.asset_id != 0
            && self.sale_price >= 0
            && self.purchase_price >= 0
            && matches!(self.outfit_type, 0 | 2 | 5 | 8 | 9 | 10 | 11)
            && self.source <= 1
            && match self.owner {
                OutfitOwner::Avatar(id) | OutfitOwner::Object(id) => id != 0,
            }
    }
    pub(super) fn save(&self, w: &mut Writer) {
        w.u32(self.outfit_id);
        w.u64(self.asset_id);
        w.i32(self.sale_price);
        w.i32(self.purchase_price);
        self.owner.save(w);
        w.u8(self.outfit_type);
        w.u8(self.source);
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let v = Self {
            outfit_id: r.u32()?,
            asset_id: r.u64()?,
            sale_price: r.i32()?,
            purchase_price: r.i32()?,
            owner: OutfitOwner::restore(r)?,
            outfit_type: r.u8()?,
            source: r.u8()?,
        };
        if !v.valid() {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(v)
    }
}
impl OutfitOwner {
    pub(super) fn save(self, w: &mut Writer) {
        match self {
            Self::Avatar(v) => {
                w.u8(1);
                w.u32(v)
            }
            Self::Object(v) => {
                w.u8(2);
                w.u32(v)
            }
        }
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let tag = r.u8()?;
        let id = r.u32()?;
        if id == 0 {
            return Err(Error::InvalidCheckpoint);
        }
        match tag {
            1 => Ok(Self::Avatar(id)),
            2 => Ok(Self::Object(id)),
            _ => Err(Error::InvalidCheckpoint),
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct CatalogOutfit {
    pub asset_id: u64,
    pub price: i32,
    pub outfit_type: u8,
}
#[derive(Clone, PartialEq, Eq)]
pub struct TradeObject {
    pub guid: u32,
    pub persistent_id: u32,
    pub data: Vec<u8>,
    pub lot_id: u32,
    pub object_count: i32,
    pub object_value: i64,
    pub lot_name: String,
}
impl TradeObject {
    pub(super) fn valid(&self) -> bool {
        self.guid != 0
            && self.persistent_id != 0
            && self.data.len() <= 16384
            && self.object_count >= 0
            && self.object_value >= 0
            && self.lot_name.len() <= 512
            && self.lot_name.encode_utf16().count() <= 128
            && (self.lot_id != 0
                || self.object_count == 0 && self.object_value == 0 && self.lot_name.is_empty())
    }
    pub(super) fn save(&self, w: &mut Writer) {
        w.u32(self.guid);
        w.u32(self.persistent_id);
        w.bytes(&self.data);
        w.u32(self.lot_id);
        w.i32(self.object_count);
        w.i64(self.object_value);
        w.string(&self.lot_name)
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let v = Self {
            guid: r.u32()?,
            persistent_id: r.u32()?,
            data: r.bytes(16384)?,
            lot_id: r.u32()?,
            object_count: r.i32()?,
            object_value: r.i64()?,
            lot_name: r.string(512)?,
        };
        if !v.valid() {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(v)
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct TradeOffer {
    pub avatar: u32,
    pub items: [Option<TradeObject>; 5],
    pub money: i32,
    pub accepted: bool,
}
impl TradeOffer {
    pub(super) fn empty(avatar: u32) -> Self {
        Self {
            avatar,
            items: std::array::from_fn(|_| None),
            money: 0,
            accepted: false,
        }
    }
    pub(super) fn valid(&self) -> bool {
        let mut ids = std::collections::BTreeSet::new();
        let mut lots = 0;
        self.avatar != 0
            && self.money >= 0
            && self.items.iter().flatten().all(|i| {
                lots += usize::from(i.lot_id != 0);
                i.valid() && ids.insert((i.lot_id, i.persistent_id))
            })
            && lots <= 1
    }
    pub(super) fn save(&self, w: &mut Writer) {
        w.u32(self.avatar);
        for v in &self.items {
            w.bool(v.is_some());
            if let Some(v) = v {
                v.save(w)
            }
        }
        w.i32(self.money);
        w.bool(self.accepted);
    }
    pub(super) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let avatar = r.u32()?;
        let mut items = std::array::from_fn(|_| None);
        for i in &mut items {
            if r.bool()? {
                *i = Some(TradeObject::restore(r)?);
            }
        }
        let v = Self {
            avatar,
            items,
            money: r.i32()?,
            accepted: r.bool()?,
        };
        if !v.valid() {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(v)
    }
}

#[derive(Clone, PartialEq, Eq)]
// Public provider requests deliberately own the two fixed five-slot offers.
// The private action queue boxes requests; host record/byte limits bound storage.
#[allow(clippy::large_enum_variant)]
pub enum Operation {
    DynamicPayouts,
    BulletinState {
        lot: u32,
    },
    /// Resolve account scope and atomically compare/update the relevant cooldown.
    /// Local rows use the original avatar/account/DateTime-ticks tuple encoding.
    Cooldown {
        object: u32,
        object_guid: u32,
        lot: u32,
        category: Option<u8>,
        local: bool,
        by_account: bool,
        avatar: u32,
        now_ticks: i64,
        duration_ticks: i64,
    },
    LoadPluginData {
        object: u32,
        plugin: PluginId,
    },
    SavePluginData {
        object: u32,
        plugin: PluginId,
        expected_revision: u64,
        bytes: Vec<u8>,
    },
    /// Atomically update owner/customer rack name namespaces under one logical revision.
    SaveRackName {
        object: u32,
        avatar: u32,
        expected_revision: u64,
        name: String,
    },
    ListOutfits {
        owner: OutfitOwner,
    },
    ReadCatalog {
        rack_type: u8,
    },
    ReadTrunkCollection {
        kind: u8,
        gender: u8,
    },
    UpdateOutfitPrice {
        object: u32,
        avatar: u32,
        outfit: u32,
        price: i32,
    },
    DeleteOutfit {
        owner: OutfitOwner,
        actor: u32,
        outfit: u32,
        keep_one_in_category: bool,
    },
    /// Debit and stock atomically; enforce source max20 and canonical catalog price.
    StockOutfit {
        object: u32,
        avatar: u32,
        asset: u64,
        price: i32,
        outfit_type: u8,
    },
    /// Debit and transfer atomically; enforce no duplicate asset and max5/category.
    PurchaseOutfit {
        object: u32,
        avatar: u32,
        outfit: u32,
        asset: u64,
        price: i32,
    },
    ReadInventory {
        avatar: u32,
        item: u32,
    },
    ReadProperty {
        avatar: u32,
        with_objects: bool,
        untradable: Vec<u32>,
    },
    CheckFunds {
        avatar: u32,
        other: u32,
        amount: i32,
    },
    /// Atomic all-or-nothing exchange with fresh ownership/funds/untradable checks.
    SecureTrade {
        offers: [TradeOffer; 2],
        untradable: Vec<u32>,
    },
}
#[derive(Clone, PartialEq, Eq)]
pub enum Reply {
    Newspaper(Vec<u8>),
    Bulletin {
        last_id: u32,
        activity: u32,
    },
    Cooldown {
        allowed: bool,
        expires_ticks: i64,
    },
    PluginData {
        exists: bool,
        revision: u64,
        bytes: Vec<u8>,
    },
    Saved {
        revision: u64,
    },
    Outfits(Vec<Outfit>),
    Catalog(Vec<CatalogOutfit>),
    Collection(Vec<u64>),
    Mutation {
        success: bool,
    },
    Inventory(Option<TradeObject>),
    Property {
        lot_id: u32,
        object_count: i32,
        object_value: i64,
        lot_name: String,
    },
    Funds {
        avatar: u32,
        other: u32,
        amount: i32,
        success: bool,
    },
    Trade {
        error: u8,
    },
    ProviderDenied,
}
macro_rules! redacted {($($t:ident),+)=>{$(impl std::fmt::Debug for $t{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str(concat!(stringify!($t),"([REDACTED])"))}})+};}
redacted!(
    Config,
    Outfit,
    CatalogOutfit,
    TradeObject,
    TradeOffer,
    Operation,
    Reply
);

fn ids(w: &mut Writer, v: &[u32]) {
    w.u32(v.len() as u32);
    for i in v {
        w.u32(*i)
    }
}
fn read_ids(r: &mut Reader<'_>) -> Result<Vec<u32>, Error> {
    let n = r.count(4096)?;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(r.u32()?)
    }
    Ok(v)
}
impl Operation {
    pub(crate) fn save(&self, w: &mut Writer) {
        match self {
            Self::DynamicPayouts => w.u8(0),
            Self::BulletinState { lot } => {
                w.u8(1);
                w.u32(*lot)
            }
            Self::Cooldown {
                object,
                object_guid,
                lot,
                category,
                local,
                by_account,
                avatar,
                now_ticks,
                duration_ticks,
            } => {
                w.u8(2);
                w.u32(*object);
                w.u32(*object_guid);
                w.u32(*lot);
                w.bool(category.is_some());
                if let Some(v) = category {
                    w.u8(*v)
                }
                w.bool(*local);
                w.bool(*by_account);
                w.u32(*avatar);
                w.i64(*now_ticks);
                w.i64(*duration_ticks)
            }
            Self::LoadPluginData { object, plugin } => {
                w.u8(3);
                w.u32(*object);
                w.u32(plugin.0)
            }
            Self::SavePluginData {
                object,
                plugin,
                expected_revision,
                bytes,
            } => {
                w.u8(4);
                w.u32(*object);
                w.u32(plugin.0);
                w.u64(*expected_revision);
                w.bytes(bytes)
            }
            Self::SaveRackName {
                object,
                avatar,
                expected_revision,
                name,
            } => {
                w.u8(5);
                w.u32(*object);
                w.u32(*avatar);
                w.u64(*expected_revision);
                w.string(name)
            }
            Self::ListOutfits { owner } => {
                w.u8(6);
                owner.save(w)
            }
            Self::ReadCatalog { rack_type } => {
                w.u8(7);
                w.u8(*rack_type)
            }
            Self::ReadTrunkCollection { kind, gender } => {
                w.u8(8);
                w.u8(*kind);
                w.u8(*gender)
            }
            Self::UpdateOutfitPrice {
                object,
                avatar,
                outfit,
                price,
            } => {
                w.u8(9);
                w.u32(*object);
                w.u32(*avatar);
                w.u32(*outfit);
                w.i32(*price)
            }
            Self::DeleteOutfit {
                owner,
                actor,
                outfit,
                keep_one_in_category,
            } => {
                w.u8(10);
                owner.save(w);
                w.u32(*actor);
                w.u32(*outfit);
                w.bool(*keep_one_in_category)
            }
            Self::StockOutfit {
                object,
                avatar,
                asset,
                price,
                outfit_type,
            } => {
                w.u8(11);
                w.u32(*object);
                w.u32(*avatar);
                w.u64(*asset);
                w.i32(*price);
                w.u8(*outfit_type)
            }
            Self::PurchaseOutfit {
                object,
                avatar,
                outfit,
                asset,
                price,
            } => {
                w.u8(12);
                w.u32(*object);
                w.u32(*avatar);
                w.u32(*outfit);
                w.u64(*asset);
                w.i32(*price)
            }
            Self::ReadInventory { avatar, item } => {
                w.u8(13);
                w.u32(*avatar);
                w.u32(*item)
            }
            Self::ReadProperty {
                avatar,
                with_objects,
                untradable,
            } => {
                w.u8(14);
                w.u32(*avatar);
                w.bool(*with_objects);
                ids(w, untradable)
            }
            Self::CheckFunds {
                avatar,
                other,
                amount,
            } => {
                w.u8(15);
                w.u32(*avatar);
                w.u32(*other);
                w.i32(*amount)
            }
            Self::SecureTrade { offers, untradable } => {
                w.u8(16);
                for o in offers {
                    o.save(w)
                }
                ids(w, untradable)
            }
        }
    }
    pub(crate) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(match r.u8()? {
            0 => Self::DynamicPayouts,
            1 => Self::BulletinState { lot: r.u32()? },
            2 => Self::Cooldown {
                object: r.u32()?,
                object_guid: r.u32()?,
                lot: r.u32()?,
                category: if r.bool()? { Some(r.u8()?) } else { None },
                local: r.bool()?,
                by_account: r.bool()?,
                avatar: r.u32()?,
                now_ticks: r.i64()?,
                duration_ticks: r.i64()?,
            },
            3 => Self::LoadPluginData {
                object: r.u32()?,
                plugin: PluginId(r.u32()?),
            },
            4 => Self::SavePluginData {
                object: r.u32()?,
                plugin: PluginId(r.u32()?),
                expected_revision: r.u64()?,
                bytes: r.bytes(256 * 1024)?,
            },
            5 => Self::SaveRackName {
                object: r.u32()?,
                avatar: r.u32()?,
                expected_revision: r.u64()?,
                name: r.string(128)?,
            },
            6 => Self::ListOutfits {
                owner: OutfitOwner::restore(r)?,
            },
            7 => Self::ReadCatalog { rack_type: r.u8()? },
            8 => Self::ReadTrunkCollection {
                kind: r.u8()?,
                gender: r.u8()?,
            },
            9 => Self::UpdateOutfitPrice {
                object: r.u32()?,
                avatar: r.u32()?,
                outfit: r.u32()?,
                price: r.i32()?,
            },
            10 => Self::DeleteOutfit {
                owner: OutfitOwner::restore(r)?,
                actor: r.u32()?,
                outfit: r.u32()?,
                keep_one_in_category: r.bool()?,
            },
            11 => Self::StockOutfit {
                object: r.u32()?,
                avatar: r.u32()?,
                asset: r.u64()?,
                price: r.i32()?,
                outfit_type: r.u8()?,
            },
            12 => Self::PurchaseOutfit {
                object: r.u32()?,
                avatar: r.u32()?,
                outfit: r.u32()?,
                asset: r.u64()?,
                price: r.i32()?,
            },
            13 => Self::ReadInventory {
                avatar: r.u32()?,
                item: r.u32()?,
            },
            14 => Self::ReadProperty {
                avatar: r.u32()?,
                with_objects: r.bool()?,
                untradable: read_ids(r)?,
            },
            15 => Self::CheckFunds {
                avatar: r.u32()?,
                other: r.u32()?,
                amount: r.i32()?,
            },
            16 => Self::SecureTrade {
                offers: [TradeOffer::restore(r)?, TradeOffer::restore(r)?],
                untradable: read_ids(r)?,
            },
            _ => return Err(Error::InvalidCheckpoint),
        })
    }
}
impl Reply {
    pub(crate) fn save(&self, w: &mut Writer) {
        match self {
            Self::Newspaper(b) => {
                w.u8(0);
                w.bytes(b)
            }
            Self::Bulletin { last_id, activity } => {
                w.u8(1);
                w.u32(*last_id);
                w.u32(*activity)
            }
            Self::Cooldown {
                allowed,
                expires_ticks,
            } => {
                w.u8(2);
                w.bool(*allowed);
                w.i64(*expires_ticks)
            }
            Self::PluginData {
                exists,
                revision,
                bytes,
            } => {
                w.u8(3);
                w.bool(*exists);
                w.u64(*revision);
                w.bytes(bytes)
            }
            Self::Saved { revision } => {
                w.u8(4);
                w.u64(*revision)
            }
            Self::Outfits(v) => {
                w.u8(5);
                w.u32(v.len() as u32);
                for o in v {
                    o.save(w)
                }
            }
            Self::Catalog(v) => {
                w.u8(6);
                w.u32(v.len() as u32);
                for o in v {
                    w.u64(o.asset_id);
                    w.i32(o.price);
                    w.u8(o.outfit_type)
                }
            }
            Self::Collection(v) => {
                w.u8(7);
                w.u32(v.len() as u32);
                for a in v {
                    w.u64(*a)
                }
            }
            Self::Mutation { success } => {
                w.u8(8);
                w.bool(*success)
            }
            Self::Inventory(v) => {
                w.u8(9);
                w.bool(v.is_some());
                if let Some(v) = v {
                    v.save(w)
                }
            }
            Self::Property {
                lot_id,
                object_count,
                object_value,
                lot_name,
            } => {
                w.u8(10);
                w.u32(*lot_id);
                w.i32(*object_count);
                w.i64(*object_value);
                w.string(lot_name)
            }
            Self::Funds {
                avatar,
                other,
                amount,
                success,
            } => {
                w.u8(11);
                w.u32(*avatar);
                w.u32(*other);
                w.i32(*amount);
                w.bool(*success)
            }
            Self::Trade { error } => {
                w.u8(12);
                w.u8(*error)
            }
            Self::ProviderDenied => w.u8(13),
        }
    }
    pub(crate) fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(match r.u8()? {
            0 => Self::Newspaper(r.bytes(256 * 1024)?),
            1 => Self::Bulletin {
                last_id: r.u32()?,
                activity: r.u32()?,
            },
            2 => Self::Cooldown {
                allowed: r.bool()?,
                expires_ticks: r.i64()?,
            },
            3 => Self::PluginData {
                exists: r.bool()?,
                revision: r.u64()?,
                bytes: r.bytes(256 * 1024)?,
            },
            4 => Self::Saved { revision: r.u64()? },
            5 => {
                let n = r.count(4096)?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(Outfit::restore(r)?)
                }
                Self::Outfits(v)
            }
            6 => {
                let n = r.count(4096)?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(CatalogOutfit {
                        asset_id: r.u64()?,
                        price: r.i32()?,
                        outfit_type: r.u8()?,
                    })
                }
                Self::Catalog(v)
            }
            7 => {
                let n = r.count(4096)?;
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(r.u64()?)
                }
                Self::Collection(v)
            }
            8 => Self::Mutation { success: r.bool()? },
            9 => Self::Inventory(if r.bool()? {
                Some(TradeObject::restore(r)?)
            } else {
                None
            }),
            10 => Self::Property {
                lot_id: r.u32()?,
                object_count: r.i32()?,
                object_value: r.i64()?,
                lot_name: r.string(512)?,
            },
            11 => Self::Funds {
                avatar: r.u32()?,
                other: r.u32()?,
                amount: r.i32()?,
                success: r.bool()?,
            },
            12 => {
                let error = r.u8()?;
                if error > 9 {
                    return Err(Error::InvalidCheckpoint);
                }
                Self::Trade { error }
            }
            13 => Self::ProviderDenied,
            _ => return Err(Error::InvalidCheckpoint),
        })
    }
}
