use crate::AuthoringError;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogItem {
    pub guid: u32,
    pub category: i8,
    pub price: u32,
    pub name: String,
    pub disable_level: u8,
    pub tags: Option<String>,
    pub thumbnail: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedOutfit {
    pub outfit_id: u32,
    #[serde(with = "decimal_u64")]
    pub asset_id: u64,
    pub sale_price: i32,
    pub purchase_price: i32,
    pub owner_type: u16,
    pub owner_id: u32,
    pub category: u8,
    pub source: u16,
    pub label: Option<String>,
    pub thumbnail: Option<String>,
}
/// Lossless JSON bridge: original ulong keys never enter JavaScript numbers.
pub mod decimal_u64 {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};
    pub fn serialize<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        let value = String::deserialize(d)?;
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err(D::Error::custom("decimal ulong key required"));
        }
        value.parse().map_err(D::Error::custom)
    }
}
pub fn parse_catalog_xml(xml: &str) -> Result<Vec<CatalogItem>, AuthoringError> {
    use quick_xml::{Reader, events::Event};
    use std::collections::BTreeMap;
    let mut reader = Reader::from_str(xml);
    let mut rows = vec![];
    loop {
        match reader
            .read_event()
            .map_err(|_| AuthoringError::Invalid("catalog XML"))?
        {
            Event::Empty(e) | Event::Start(e) if e.name().as_ref() == b"P" => {
                let mut fields = BTreeMap::new();
                for a in e.attributes() {
                    let a = a.map_err(|_| AuthoringError::Invalid("catalog attribute"))?;
                    let key = std::str::from_utf8(a.key.as_ref())
                        .map_err(|_| AuthoringError::Invalid("catalog key"))?
                        .to_string();
                    let value = a
                        .decode_and_unescape_value(reader.decoder())
                        .map_err(|_| AuthoringError::Invalid("catalog value"))?
                        .into_owned();
                    fields.insert(key, value);
                }
                let get = |key: &str| {
                    fields
                        .get(key)
                        .ok_or(AuthoringError::Missing("catalog attribute"))
                };
                let category: i8 = get("s")?
                    .parse()
                    .map_err(|_| AuthoringError::Invalid("catalog category"))?;
                if category < 0 {
                    continue;
                }
                let guid = u32::from_str_radix(
                    get("g")?.trim_start_matches("0x").trim_start_matches("0X"),
                    16,
                )
                .map_err(|_| AuthoringError::Invalid("catalog GUID"))?;
                let price = get("p")?
                    .parse()
                    .map_err(|_| AuthoringError::Invalid("catalog price"))?;
                let disable_level = fields
                    .get("r")
                    .map(|s| s.parse())
                    .transpose()
                    .map_err(|_| AuthoringError::Invalid("catalog disable level"))?
                    .unwrap_or(0);
                rows.push(CatalogItem {
                    guid,
                    category,
                    price,
                    name: get("n")?.clone(),
                    disable_level,
                    tags: fields.get("t").cloned(),
                    thumbnail: None,
                });
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(rows)
}
/// VMEODRackStockResponse / VMGLOutfit use IoBuffer big endian, enum fields u16.
pub fn decode_outfit_stock(bytes: &[u8]) -> Result<Vec<OwnedOutfit>, AuthoringError> {
    use crate::codec::Reader;
    let mut r = Reader { bytes, at: 0 };
    let count = i32::from_be_bytes(r.take(4)?.try_into().unwrap());
    if count < 0
        || count as usize != (bytes.len() - 4) / 29
        || !(bytes.len() - 4).is_multiple_of(29)
    {
        return Err(AuthoringError::Invalid("outfit stock count"));
    }
    let mut rows = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let outfit_id = u32::from_be_bytes(r.take(4)?.try_into().unwrap());
        let asset_id = u64::from_be_bytes(r.take(8)?.try_into().unwrap());
        let sale_price = i32::from_be_bytes(r.take(4)?.try_into().unwrap());
        let purchase_price = i32::from_be_bytes(r.take(4)?.try_into().unwrap());
        let owner_type = u16::from_be_bytes(r.take(2)?.try_into().unwrap());
        let owner_id = u32::from_be_bytes(r.take(4)?.try_into().unwrap());
        let category = r.u8()?;
        let source = u16::from_be_bytes(r.take(2)?.try_into().unwrap());
        if !(1..=2).contains(&owner_type) || source > 1 {
            return Err(AuthoringError::Invalid("outfit owner/source"));
        }
        rows.push(OwnedOutfit {
            outfit_id,
            asset_id,
            sale_price,
            purchase_price,
            owner_type,
            owner_id,
            category,
            source,
            label: None,
            thumbnail: None,
        });
    }
    Ok(rows)
}
/// Source VMNetBuyObjectCmd calculation; this is a preview, never an admission.
/// Authoritative quoted amounts/balance come from the actual VM/global link.
pub fn source_purchase_price(price: i32, discount: u8, mode: u8) -> Result<i32, AuthoringError> {
    if price < 0 || discount > 100 || !(1..=2).contains(&mode) {
        return Err(AuthoringError::Price);
    }
    let mut value = i64::from(price) * i64::from(100 - discount) / 100;
    if mode == 2 {
        value -= value * 2 / 3;
    }
    i32::try_from(value).map_err(|_| AuthoringError::Price)
}
