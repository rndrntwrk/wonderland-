//! Private slice-backed bincode guard for the known AcceptedTick schema.
//! Account for nested work before handing allocation hints to Serde containers.
use serde::de::{DeserializeSeed, EnumAccess, Error, MapAccess, SeqAccess, VariantAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::{cell::Cell, fmt, marker::PhantomData};

pub(super) struct Budget {
    items: Cell<usize>,
    credit: Cell<usize>,
    depth: usize,
}
impl Budget {
    pub(super) fn new(items: usize, credit: usize, depth: usize) -> Self {
        Self {
            items: Cell::new(items),
            credit: Cell::new(credit),
            depth,
        }
    }
    fn claim<E: Error>(&self, count: usize, bytes: usize) -> Result<(), E> {
        let items = self
            .items
            .get()
            .checked_sub(count)
            .ok_or_else(|| E::custom("wire decode work limit"))?;
        let credit = self
            .credit
            .get()
            .checked_sub(bytes)
            .ok_or_else(|| E::custom("wire decode allocation credit"))?;
        self.items.set(items);
        self.credit.set(credit);
        Ok(())
    }
    fn value<T, E: Error>(&self) -> Result<(), E> {
        let bytes = std::mem::size_of::<T>()
            .checked_mul(4)
            .and_then(|n| n.checked_add(64))
            .ok_or_else(|| E::custom("wire value size overflow"))?;
        self.claim(1, bytes)
    }
    fn bytes<E: Error>(&self, len: usize) -> Result<(), E> {
        let bytes = len
            .checked_mul(4)
            .and_then(|n| n.checked_add(64))
            .ok_or_else(|| E::custom("wire byte size overflow"))?;
        self.claim(1, bytes)
    }
    fn hint<E: Error>(&self, hint: Option<usize>) -> Result<(), E> {
        if hint.is_some_and(|n| n > self.items.get()) {
            return Err(E::custom("wire collection hint exceeds remaining work"));
        }
        self.claim(1, 64)
    }
}

pub(super) struct ValueSeed<'a, T> {
    budget: &'a Budget,
    marker: PhantomData<T>,
}
impl<'a, T> ValueSeed<'a, T> {
    pub(super) fn new(budget: &'a Budget) -> Self {
        Self {
            budget,
            marker: PhantomData,
        }
    }
}
impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for ValueSeed<'_, T> {
    type Value = T;
    fn deserialize<D: Deserializer<'de>>(self, inner: D) -> Result<T, D::Error> {
        self.budget.value::<T, D::Error>()?;
        T::deserialize(Bounded {
            inner,
            depth: 0,
            budget: self.budget,
        })
    }
}
struct Bounded<'a, D> {
    inner: D,
    depth: usize,
    budget: &'a Budget,
}
struct Seed<'a, S> {
    inner: S,
    depth: usize,
    budget: &'a Budget,
}
impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Seed<'_, S> {
    type Value = S::Value;
    fn deserialize<D: Deserializer<'de>>(self, inner: D) -> Result<Self::Value, D::Error> {
        self.budget.value::<S::Value, D::Error>()?;
        self.inner.deserialize(Bounded {
            inner,
            depth: self.depth,
            budget: self.budget,
        })
    }
}
struct GuardedVisitor<'a, V> {
    inner: V,
    depth: usize,
    budget: &'a Budget,
}
macro_rules! scalars { ($($method:ident($ty:ty)),* $(,)?)=>{$(
    fn $method<E:Error>(self,v:$ty)->Result<Self::Value,E>{self.inner.$method(v)}
)*}; }
impl<'de, V: Visitor<'de>> Visitor<'de> for GuardedVisitor<'_, V> {
    type Value = V::Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.expecting(f)
    }
    scalars!(
        visit_bool(bool),
        visit_i8(i8),
        visit_i16(i16),
        visit_i32(i32),
        visit_i64(i64),
        visit_i128(i128),
        visit_u8(u8),
        visit_u16(u16),
        visit_u32(u32),
        visit_u64(u64),
        visit_u128(u128),
        visit_f32(f32),
        visit_f64(f64),
        visit_char(char)
    );
    fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
        self.budget.bytes::<E>(v.len())?;
        self.inner.visit_str(v)
    }
    fn visit_borrowed_str<E: Error>(self, v: &'de str) -> Result<Self::Value, E> {
        self.budget.bytes::<E>(v.len())?;
        self.inner.visit_borrowed_str(v)
    }
    fn visit_string<E: Error>(self, v: String) -> Result<Self::Value, E> {
        self.budget.bytes::<E>(v.len())?;
        self.inner.visit_string(v)
    }
    fn visit_bytes<E: Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        self.budget.bytes::<E>(v.len())?;
        self.inner.visit_bytes(v)
    }
    fn visit_borrowed_bytes<E: Error>(self, v: &'de [u8]) -> Result<Self::Value, E> {
        self.budget.bytes::<E>(v.len())?;
        self.inner.visit_borrowed_bytes(v)
    }
    fn visit_byte_buf<E: Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
        self.budget.bytes::<E>(v.len())?;
        self.inner.visit_byte_buf(v)
    }
    fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_unit()
    }
    fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_none()
    }
    fn visit_some<D: Deserializer<'de>>(self, inner: D) -> Result<Self::Value, D::Error> {
        self.inner.visit_some(Bounded {
            inner,
            depth: self.depth + 1,
            budget: self.budget,
        })
    }
    fn visit_newtype_struct<D: Deserializer<'de>>(self, inner: D) -> Result<Self::Value, D::Error> {
        self.inner.visit_newtype_struct(Bounded {
            inner,
            depth: self.depth + 1,
            budget: self.budget,
        })
    }
    fn visit_seq<A: SeqAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.budget.hint::<A::Error>(inner.size_hint())?;
        self.inner.visit_seq(Sequence {
            inner,
            depth: self.depth + 1,
            budget: self.budget,
        })
    }
    fn visit_map<A: MapAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.budget.hint::<A::Error>(inner.size_hint())?;
        self.inner.visit_map(Map {
            inner,
            depth: self.depth + 1,
            budget: self.budget,
        })
    }
    fn visit_enum<A: EnumAccess<'de>>(self, inner: A) -> Result<Self::Value, A::Error> {
        self.inner.visit_enum(Enum {
            inner,
            depth: self.depth + 1,
            budget: self.budget,
        })
    }
}
struct Sequence<'a, A> {
    inner: A,
    depth: usize,
    budget: &'a Budget,
}
impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for Sequence<'_, A> {
    type Error = A::Error;
    fn next_element_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, A::Error> {
        self.inner.next_element_seed(Seed {
            inner: seed,
            depth: self.depth,
            budget: self.budget,
        })
    }
    // Never let an attacker-controlled hint reserve capacity before value admission.
    fn size_hint(&self) -> Option<usize> {
        None
    }
}
struct Map<'a, A> {
    inner: A,
    depth: usize,
    budget: &'a Budget,
}
impl<'de, A: MapAccess<'de>> MapAccess<'de> for Map<'_, A> {
    type Error = A::Error;
    fn next_key_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, A::Error> {
        self.inner.next_key_seed(Seed {
            inner: seed,
            depth: self.depth,
            budget: self.budget,
        })
    }
    fn next_value_seed<S: DeserializeSeed<'de>>(&mut self, seed: S) -> Result<S::Value, A::Error> {
        self.inner.next_value_seed(Seed {
            inner: seed,
            depth: self.depth,
            budget: self.budget,
        })
    }
    fn size_hint(&self) -> Option<usize> {
        None
    }
}
struct Enum<'a, A> {
    inner: A,
    depth: usize,
    budget: &'a Budget,
}
impl<'de, 'a, A: EnumAccess<'de>> EnumAccess<'de> for Enum<'a, A> {
    type Error = A::Error;
    type Variant = Variant<'a, A::Variant>;
    fn variant_seed<S: DeserializeSeed<'de>>(
        self,
        seed: S,
    ) -> Result<(S::Value, Self::Variant), A::Error> {
        let (value, inner) = self.inner.variant_seed(Seed {
            inner: seed,
            depth: self.depth,
            budget: self.budget,
        })?;
        Ok((
            value,
            Variant {
                inner,
                depth: self.depth,
                budget: self.budget,
            },
        ))
    }
}
struct Variant<'a, A> {
    inner: A,
    depth: usize,
    budget: &'a Budget,
}
impl<'de, A: VariantAccess<'de>> VariantAccess<'de> for Variant<'_, A> {
    type Error = A::Error;
    fn unit_variant(self) -> Result<(), A::Error> {
        self.inner.unit_variant()
    }
    fn newtype_variant_seed<S: DeserializeSeed<'de>>(self, seed: S) -> Result<S::Value, A::Error> {
        self.inner.newtype_variant_seed(Seed {
            inner: seed,
            depth: self.depth + 1,
            budget: self.budget,
        })
    }
    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, A::Error> {
        self.inner.tuple_variant(
            len,
            GuardedVisitor {
                inner: visitor,
                depth: self.depth + 1,
                budget: self.budget,
            },
        )
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, A::Error> {
        self.inner.struct_variant(
            fields,
            GuardedVisitor {
                inner: visitor,
                depth: self.depth + 1,
                budget: self.budget,
            },
        )
    }
}
macro_rules! delegate {
    ($($method:ident $(($($name:ident:$ty:ty),*))?),* $(,)?)=>{$(
        fn $method<V:Visitor<'de>>(self,$($($name:$ty,)*)?visitor:V)->Result<V::Value,D::Error>{
            if self.depth>self.budget.depth{return Err(D::Error::custom("wire nesting limit"));}
            self.budget.claim::<D::Error>(1,0)?;
            self.inner.$method($($($name,)*)?GuardedVisitor {inner:visitor,depth:self.depth,budget:self.budget})
        }
    )*};
}
impl<'de, D: Deserializer<'de>> Deserializer<'de> for Bounded<'_, D> {
    type Error = D::Error;
    delegate!(
        deserialize_any,
        deserialize_bool,
        deserialize_i8,
        deserialize_i16,
        deserialize_i32,
        deserialize_i64,
        deserialize_i128,
        deserialize_u8,
        deserialize_u16,
        deserialize_u32,
        deserialize_u64,
        deserialize_u128,
        deserialize_f32,
        deserialize_f64,
        deserialize_char,
        deserialize_str,
        deserialize_bytes,
        deserialize_option,
        deserialize_unit,
        deserialize_unit_struct(name: &'static str),
        deserialize_newtype_struct(name: &'static str),
        deserialize_seq,
        deserialize_tuple(len: usize),
        deserialize_tuple_struct(name: &'static str, len: usize),
        deserialize_map,
        deserialize_struct(name: &'static str, fields: &'static [&'static str]),
        deserialize_enum(name: &'static str, variants: &'static [&'static str]),
        deserialize_identifier,
        deserialize_ignored_any
    );
    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
        self.deserialize_str(visitor)
    }
    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
        self.deserialize_bytes(visitor)
    }
    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::Options;
    use serde::Deserialize;
    fn decode<'a, T: Deserialize<'a>>(raw: &'a [u8], budget: &Budget) -> Result<T, bincode::Error> {
        bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_limit(raw.len() as u64)
            .reject_trailing_bytes()
            .deserialize_seed(ValueSeed::new(budget), raw)
    }
    #[test]
    fn zero_sized_element_bomb_is_rejected_by_hint_before_looping() {
        let budget = Budget::new(128, 8192, 16);
        assert!(
            decode::<Vec<()>>(&u64::MAX.to_le_bytes(), &budget)
                .unwrap_err()
                .to_string()
                .contains("collection hint")
        );
    }
    #[derive(serde::Serialize, Deserialize, Debug)]
    struct Nested(Vec<Nested>);
    #[test]
    fn recursive_sequence_depth_is_bounded() {
        let mut value = Nested(vec![]);
        for _ in 0..50 {
            value = Nested(vec![value]);
        }
        let raw = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .serialize(&value)
            .unwrap();
        let budget = Budget::new(65536, 1024 * 1024, 20);
        assert!(
            decode::<Nested>(&raw, &budget)
                .unwrap_err()
                .to_string()
                .contains("nesting limit")
        );
    }
    #[test]
    fn aggregate_budget_is_not_reset_between_records() {
        let raw = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .serialize(&vec![1u8; 10])
            .unwrap();
        let budget = Budget::new(40, 65536, 64);
        assert!(decode::<Vec<u8>>(&raw, &budget).is_ok());
        assert!(decode::<Vec<u8>>(&raw, &budget).is_err());
    }
    #[test]
    fn strings_are_charged_before_owned_output() {
        let raw = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .serialize(&"x".repeat(1000))
            .unwrap();
        let budget = Budget::new(100, 1024, 64);
        assert!(
            decode::<String>(&raw, &budget)
                .unwrap_err()
                .to_string()
                .contains("allocation credit")
        );
    }
}
