//! Read the existing tagged WLB1 model records without `deserialize_any`.
//!
//! Serialization remains the public model's original tagged layout. JSON still
//! uses its original map decoder. The positional binary reader consumes only
//! the fields named by the validated tag; it does not invent a variant index or
//! change the bytes emitted by older producers. Outer packet limits/canonical
//! comparison continue to apply in NativePlayer.
use crate::{ModelContext, ModelTextureSelector};
use serde::{
    Deserialize, Deserializer,
    de::{Error, SeqAccess, Visitor},
};
use std::fmt;
use wonderland_render_core::AssetKey;
#[derive(Deserialize)]
#[serde(remote = "ModelContext", tag = "kind", rename_all = "snake_case")]
enum ContextJson {
    Dgrp {
        effective_iff: AssetKey,
        chunk_id: u16,
    },
    Standalone,
    Vitaboy,
}
#[derive(Deserialize)]
#[serde(
    remote = "ModelTextureSelector",
    tag = "kind",
    rename_all = "snake_case"
)]
enum SelectorJson {
    Sprite { rotation: u16, ordinal: u16 },
    Custom { id: u16 },
}
enum Tag {
    Dgrp,
    Standalone,
    Vitaboy,
    Sprite,
    Custom,
}
impl<'de> Deserialize<'de> for Tag {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Tags;
        impl Visitor<'_> for Tags {
            type Value = Tag;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an original binary model tag")
            }
            fn visit_str<E: Error>(self, s: &str) -> Result<Tag, E> {
                match s {
                    "dgrp" => Ok(Tag::Dgrp),
                    "standalone" => Ok(Tag::Standalone),
                    "vitaboy" => Ok(Tag::Vitaboy),
                    "sprite" => Ok(Tag::Sprite),
                    "custom" => Ok(Tag::Custom),
                    _ => Err(E::custom("Unknown binary model tag")),
                }
            }
        }
        d.deserialize_str(Tags)
    }
}
fn field<'de, A: SeqAccess<'de>, T: Deserialize<'de>>(s: &mut A) -> Result<T, A::Error> {
    s.next_element()?
        .ok_or_else(|| A::Error::custom("Truncated binary model record"))
}
impl<'de> Deserialize<'de> for ModelContext {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if d.is_human_readable() {
            return ContextJson::deserialize(d);
        }
        struct Context;
        impl<'de> Visitor<'de> for Context {
            type Value = ModelContext;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("the original tagged model context")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Self::Value, A::Error> {
                match field(&mut s)? {
                    Tag::Dgrp => Ok(ModelContext::Dgrp {
                        effective_iff: field(&mut s)?,
                        chunk_id: field(&mut s)?,
                    }),
                    Tag::Standalone => Ok(ModelContext::Standalone),
                    Tag::Vitaboy => Ok(ModelContext::Vitaboy),
                    _ => Err(A::Error::custom("Invalid binary model context tag")),
                }
            }
        }
        d.deserialize_struct(
            "ModelContext",
            &["kind", "effective_iff", "chunk_id"],
            Context,
        )
    }
}
impl<'de> Deserialize<'de> for ModelTextureSelector {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if d.is_human_readable() {
            return SelectorJson::deserialize(d);
        }
        struct Selector;
        impl<'de> Visitor<'de> for Selector {
            type Value = ModelTextureSelector;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("the original tagged texture selector")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Self::Value, A::Error> {
                match field(&mut s)? {
                    Tag::Sprite => Ok(ModelTextureSelector::Sprite {
                        rotation: field(&mut s)?,
                        ordinal: field(&mut s)?,
                    }),
                    Tag::Custom => Ok(ModelTextureSelector::Custom { id: field(&mut s)? }),
                    _ => Err(A::Error::custom("Invalid binary texture selector tag")),
                }
            }
        }
        d.deserialize_struct(
            "ModelTextureSelector",
            &["kind", "first", "second"],
            Selector,
        )
    }
}
