// SPDX-License-Identifier: MPL-2.0
//! Exact avatar reference layouts from PurchasableOutfit.cs, Collection.cs and
//! HandGroup.cs at the pinned original source. These are identifiers, not asset
//! discovery guesses: dependency resolution remains explicit in the cooker.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchasableOutfit {
    pub version: u32,
    pub gender: u32,
    pub asset_id_size: u32,
    /// The original reader ignores this word; all four source bytes survive.
    pub asset_prefix: [u8; 4],
    pub outfit_id: u64,
}

pub fn decode_purchasable_outfit(bytes: &[u8], limits: &Limits) -> Result<PurchasableOutfit> {
    DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let value = PurchasableOutfit {
        version: r.u32_be()?,
        gender: r.u32_be()?,
        asset_id_size: r.u32_be()?,
        asset_prefix: r.read_bytes(4)?.try_into().expect("fixed-size read"),
        outfit_id: u64::from_be_bytes(r.read_bytes(8)?.try_into().expect("fixed-size read")),
    };
    // These fields do not select a source layout. Preserve unusual declarations
    // while consuming the same fixed 64-bit reference that the source consumes.
    finish(&r)?;
    Ok(value)
}

pub fn encode_purchasable_outfit(value: &PurchasableOutfit, limits: &Limits) -> Result<Vec<u8>> {
    encode::encode(limits, |w| {
        w.u32(value.version)?;
        w.u32(value.gender)?;
        w.u32(value.asset_id_size)?;
        w.put(&value.asset_prefix)?;
        w.put(&value.outfit_id.to_be_bytes())
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionItem {
    pub index: i32,
    pub outfit: FileKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    pub items: Vec<CollectionItem>,
}

pub fn decode_collection(bytes: &[u8], limits: &Limits) -> Result<Collection> {
    let mut budget = DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let n = signed_count(&mut r, &mut budget, limits.max_entries, "collection items")?;
    budget.reserve::<CollectionItem>(n, limits.max_entries, 0, "collection items")?;
    let mut items = Vec::with_capacity(n);
    for _ in 0..n {
        items.push(CollectionItem {
            index: r.i32_be()?,
            outfit: file_key(&mut r)?,
        });
    }
    finish(&r)?;
    Ok(Collection { items })
}

pub fn encode_collection(value: &Collection, limits: &Limits) -> Result<Vec<u8>> {
    encode::encode(limits, |w| {
        w.retain::<CollectionItem>(value.items.capacity())?;
        w.count(
            value.items.len(),
            limits.max_entries.min(i32::MAX as usize),
            "collection items",
        )?;
        for item in &value.items {
            w.i32(item.index)?;
            w.file_key(item.outfit)?;
        }
        Ok(())
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandGroup {
    pub version: u32,
    /// Light, medium, dark skin; right then left hand; idle, fist, pointing.
    pub appearances: [FileKey; 18],
}

pub fn decode_hand_group(bytes: &[u8], limits: &Limits) -> Result<HandGroup> {
    DecodeBudget::new(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = r.u32_be()?;
    let mut appearances = [FileKey {
        file_id: 0,
        type_id: 0,
    }; 18];
    for key in &mut appearances {
        *key = file_key(&mut r)?;
    }
    finish(&r)?;
    Ok(HandGroup {
        version,
        appearances,
    })
}

pub fn encode_hand_group(value: &HandGroup, limits: &Limits) -> Result<Vec<u8>> {
    encode::encode(limits, |w| {
        w.u32(value.version)?;
        for &key in &value.appearances {
            w.file_key(key)?;
        }
        Ok(())
    })
}
