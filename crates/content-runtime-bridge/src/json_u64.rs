//! A borrowing serializer adapter: every u64 (including nested ones) is an
//! exact decimal string. No intermediate Value tree or formatting allocation is
//! required, so counting and writing use the identical bounded representation.
use serde::{ser, Serialize, Serializer};

pub(crate) struct ExactU64<'a, T: ?Sized>(pub &'a T);
impl<T: ?Sized + Serialize> Serialize for ExactU64<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(Exact(serializer))
    }
}
struct Exact<S>(S);

macro_rules! scalar {
    ($name:ident, $kind:ty) => {
        fn $name(self, value: $kind) -> Result<Self::Ok, Self::Error> {
            self.0.$name(value)
        }
    };
}
impl<S: Serializer> Serializer for Exact<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = Exact<S::SerializeSeq>;
    type SerializeTuple = Exact<S::SerializeTuple>;
    type SerializeTupleStruct = Exact<S::SerializeTupleStruct>;
    type SerializeTupleVariant = Exact<S::SerializeTupleVariant>;
    type SerializeMap = Exact<S::SerializeMap>;
    type SerializeStruct = Exact<S::SerializeStruct>;
    type SerializeStructVariant = Exact<S::SerializeStructVariant>;
    scalar!(serialize_bool, bool);
    scalar!(serialize_i8, i8);
    scalar!(serialize_i16, i16);
    scalar!(serialize_i32, i32);
    scalar!(serialize_i64, i64);
    scalar!(serialize_i128, i128);
    scalar!(serialize_u8, u8);
    scalar!(serialize_u16, u16);
    scalar!(serialize_u32, u32);
    scalar!(serialize_u128, u128);
    scalar!(serialize_f32, f32);
    scalar!(serialize_f64, f64);
    scalar!(serialize_char, char);
    scalar!(serialize_str, &str);
    scalar!(serialize_bytes, &[u8]);
    fn serialize_u64(self, mut value: u64) -> Result<Self::Ok, Self::Error> {
        let mut digits = [b'0'; 20];
        let mut start = digits.len();
        loop {
            start -= 1;
            digits[start] += (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        self.0
            .serialize_str(std::str::from_utf8(&digits[start..]).expect("decimal ASCII"))
    }
    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_none()
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_some(&ExactU64(value))
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit()
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit_struct(name)
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit_variant(name, index, variant)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_newtype_struct(name, &ExactU64(value))
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0
            .serialize_newtype_variant(name, index, variant, &ExactU64(value))
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        self.0.serialize_seq(len).map(Exact)
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.0.serialize_tuple(len).map(Exact)
    }
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.0.serialize_tuple_struct(name, len).map(Exact)
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        self.0
            .serialize_tuple_variant(name, index, variant, len)
            .map(Exact)
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        self.0.serialize_map(len).map(Exact)
    }
    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.0.serialize_struct(name, len).map(Exact)
    }
    fn serialize_struct_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        self.0
            .serialize_struct_variant(name, index, variant, len)
            .map(Exact)
    }
    fn collect_str<T: ?Sized + std::fmt::Display>(
        self,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.collect_str(value)
    }
    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

macro_rules! sequence {
    ($kind:ident, $element:ident) => {
        impl<S: ser::$kind> ser::$kind for Exact<S> {
            type Ok = S::Ok;
            type Error = S::Error;
            fn $element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
                self.0.$element(&ExactU64(value))
            }
            fn end(self) -> Result<Self::Ok, Self::Error> {
                self.0.end()
            }
        }
    };
}
sequence!(SerializeSeq, serialize_element);
sequence!(SerializeTuple, serialize_element);
sequence!(SerializeTupleStruct, serialize_field);
sequence!(SerializeTupleVariant, serialize_field);

impl<S: ser::SerializeMap> ser::SerializeMap for Exact<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.0.serialize_key(&ExactU64(value))
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.0.serialize_value(&ExactU64(value))
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}
macro_rules! fields {
    ($kind:ident) => {
        impl<S: ser::$kind> ser::$kind for Exact<S> {
            type Ok = S::Ok;
            type Error = S::Error;
            fn serialize_field<T: ?Sized + Serialize>(
                &mut self,
                name: &'static str,
                value: &T,
            ) -> Result<(), Self::Error> {
                self.0.serialize_field(name, &ExactU64(value))
            }
            fn skip_field(&mut self, name: &'static str) -> Result<(), Self::Error> {
                self.0.skip_field(name)
            }
            fn end(self) -> Result<Self::Ok, Self::Error> {
                self.0.end()
            }
        }
    };
}
fields!(SerializeStruct);
fields!(SerializeStructVariant);

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::vm::VmStop;
    #[test]
    fn nested_waiting_id_arrays_and_maps_retain_exact_decimal_values() {
        #[derive(Serialize)]
        struct View {
            stop: VmStop,
            values: [u64; 3],
            keys: std::collections::BTreeMap<u64, Option<u64>>,
            generation: u32,
            register: i16,
        }
        let view = View {
            stop: VmStop::Waiting {
                request_id: u64::MAX,
            },
            values: [0, 9_007_199_254_740_993, u64::MAX],
            keys: [(u64::MAX, Some(9_007_199_254_740_993))].into(),
            generation: u32::MAX,
            register: -1,
        };
        assert_eq!(serde_json::to_string(&ExactU64(&view)).unwrap(), "{\"stop\":{\"Waiting\":{\"request_id\":\"18446744073709551615\"}},\"values\":[\"0\",\"9007199254740993\",\"18446744073709551615\"],\"keys\":{\"18446744073709551615\":\"9007199254740993\"},\"generation\":4294967295,\"register\":-1}");
    }
}
