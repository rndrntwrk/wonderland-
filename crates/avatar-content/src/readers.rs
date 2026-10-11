//! Additional original resource readers. Layouts are from FreeSO revision
//! 60c6e823b8dc262d1c1f8a516970fb9b51fc516d, Collection.cs,
//! PurchasableOutfit.cs, HandGroup.cs and IoBuffer.cs. IoBuffer's default is BE.
use wonderland_avatar_view::{FileKey, HandGroup, HandPair, HandSet};
use wonderland_legacy_formats::{reader::Reader, Error, ErrorKind, Limits, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionItem {
    pub index: i32,
    pub purchasable: FileKey,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurchasableOutfit {
    pub version: u32,
    pub gender: u32,
    /// Retain the four opaque prefix bytes, without inventing their semantics.
    pub prefix: [u8; 4],
    pub outfit: FileKey,
    /// Official version-2 files contain four further opaque bytes that the
    /// original reader leaves unread. Preserve that observed extension exactly.
    pub trailer: Option<[u8; 4]>,
}
fn invalid(at: usize, text: &str) -> Error {
    Error::new(ErrorKind::InvalidData, at, text)
}
fn finish(r: &Reader<'_>) -> Result<()> {
    if r.remaining() != 0 {
        return Err(invalid(r.position(), "trailing resource bytes"));
    }
    Ok(())
}
fn check(bytes: &[u8], limits: &Limits) -> Result<()> {
    limits.check_input(bytes)?;
    limits.check_count(bytes.len(), limits.max_resource_bytes, 0, "resource bytes")
}
fn key(r: &mut Reader<'_>) -> Result<FileKey> {
    Ok(FileKey {
        file_id: r.u32_be()?,
        type_id: r.u32_be()?,
    })
}
pub fn decode_collection(bytes: &[u8], limits: &Limits) -> Result<Vec<CollectionItem>> {
    check(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let n = r.i32_be()?;
    if n < 0 {
        return Err(invalid(0, "negative collection count"));
    }
    let n = n as usize;
    limits.check_count(n, limits.max_entries, 0, "collection entries")?;
    let size = n
        .checked_mul(12)
        .ok_or_else(|| invalid(0, "collection size overflow"))?;
    if size != r.remaining() {
        return Err(invalid(4, "collection record length"));
    }
    limits.check_count(
        n.saturating_mul(std::mem::size_of::<CollectionItem>()),
        limits.max_total_decoded_bytes,
        0,
        "collection decoded bytes",
    )?;
    let mut items = Vec::with_capacity(n);
    for _ in 0..n {
        items.push(CollectionItem {
            index: r.i32_be()?,
            purchasable: key(&mut r)?,
        });
    }
    finish(&r)?;
    Ok(items)
}
pub fn decode_purchasable(bytes: &[u8], limits: &Limits) -> Result<PurchasableOutfit> {
    check(bytes, limits)?;
    let mut r = Reader::new(bytes);
    let version = r.u32_be()?;
    let gender = r.u32_be()?;
    if r.u32_be()? != 8 {
        return Err(invalid(8, "purchasable asset ID size must be 8"));
    }
    let prefix = r.read_bytes(4)?.try_into().expect("checked length");
    let packed = r.u64_be()?;
    let outfit = FileKey {
        file_id: (packed >> 32) as u32,
        type_id: packed as u32,
    };
    let trailer = if version == 2 && r.remaining() == 4 {
        Some(r.read_bytes(4)?.try_into().expect("checked length"))
    } else {
        None
    };
    finish(&r)?;
    // Source accepts arbitrary version/gender; retain them rather than fabricate compatibility.
    Ok(PurchasableOutfit {
        version,
        gender,
        prefix,
        outfit,
        trailer,
    })
}
pub fn decode_hand_group(bytes: &[u8], limits: &Limits) -> Result<HandGroup> {
    check(bytes, limits)?;
    let mut r = Reader::new(bytes);
    if r.u32_be()? != 1 {
        return Err(invalid(0, "unsupported hand group version"));
    }
    fn hand(r: &mut Reader<'_>) -> Result<HandSet> {
        Ok(HandSet {
            idle: key(r)?,
            fist: key(r)?,
            pointing: key(r)?,
        })
    }
    fn pair(r: &mut Reader<'_>) -> Result<HandPair> {
        Ok(HandPair {
            right: hand(r)?,
            left: hand(r)?,
        })
    }
    let result = HandGroup {
        skins: [pair(&mut r)?, pair(&mut r)?, pair(&mut r)?],
    };
    finish(&r)?;
    Ok(result)
}
