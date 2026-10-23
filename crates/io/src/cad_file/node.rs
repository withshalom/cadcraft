//! The value tree between CADCraft's model and the tags of a `.cad` record (docs/cad-format.md
//! §5): the model's serde form, with points, handles and raw DXF groups as values of their own.
//!
//! [`to_node`] serialises any model value into a [`Node`]; a node deserialises into any model
//! type (`T::deserialize(node)`), much as `serde_json::Value` does, but keeping integer and real
//! values apart and reals bit for bit.

use std::fmt;

use serde::de::{self, IntoDeserializer, Visitor};
use serde::ser::{self, Serialize};

/// One value of the model's serde form.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Node {
    None,
    Unit,
    Bool(bool),
    Int(i64),
    /// A handle, or an unsigned value above `i64::MAX`.
    U64(u64),
    F64(f64),
    Str(String),
    Bytes(Vec<u8>),
    P2(f64, f64),
    P3(f64, f64, f64),
    /// A raw DXF group (`RawTag`) with a code below 2000.
    Raw(u16, String),
    Seq(Vec<Node>),
    /// A struct, or a map whose keys are strings.
    Map(Vec<(String, Node)>),
    /// A map with keys of other kinds.
    MapAny(Vec<(Node, Node)>),
}

/// A value that doesn't fit the model, or a model value the format can't hold.
#[derive(Debug)]
pub(crate) struct NodeError(pub String);

impl fmt::Display for NodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for NodeError {}
impl ser::Error for NodeError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        NodeError(msg.to_string())
    }
}
impl de::Error for NodeError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        NodeError(msg.to_string())
    }
}

type R<T> = Result<T, NodeError>;

/// The serde form of `v` as a node.
pub(crate) fn to_node<T: Serialize + ?Sized>(v: &T) -> R<Node> {
    v.serialize(NodeSer)
}

/// A node deserialised into `T`.
pub(crate) fn from_node<T: de::DeserializeOwned>(n: Node) -> R<T> {
    T::deserialize(n)
}

// ---------------------------------------------------------------------------
// Serialising
// ---------------------------------------------------------------------------

struct NodeSer;

fn int(v: i64) -> R<Node> {
    Ok(Node::Int(v))
}

impl ser::Serializer for NodeSer {
    type Ok = Node;
    type Error = NodeError;
    type SerializeSeq = SeqSer;
    type SerializeTuple = SeqSer;
    type SerializeTupleStruct = SeqSer;
    type SerializeTupleVariant = VariantSer<SeqSer>;
    type SerializeMap = MapSer;
    type SerializeStruct = StructSer;
    type SerializeStructVariant = VariantSer<StructSer>;

    fn serialize_bool(self, v: bool) -> R<Node> {
        Ok(Node::Bool(v))
    }
    fn serialize_i8(self, v: i8) -> R<Node> {
        int(v.into())
    }
    fn serialize_i16(self, v: i16) -> R<Node> {
        int(v.into())
    }
    fn serialize_i32(self, v: i32) -> R<Node> {
        int(v.into())
    }
    fn serialize_i64(self, v: i64) -> R<Node> {
        int(v)
    }
    fn serialize_u8(self, v: u8) -> R<Node> {
        int(v.into())
    }
    fn serialize_u16(self, v: u16) -> R<Node> {
        int(v.into())
    }
    fn serialize_u32(self, v: u32) -> R<Node> {
        int(v.into())
    }
    fn serialize_u64(self, v: u64) -> R<Node> {
        Ok(i64::try_from(v).map_or(Node::U64(v), Node::Int))
    }
    fn serialize_f32(self, v: f32) -> R<Node> {
        Ok(Node::F64(v.into()))
    }
    fn serialize_f64(self, v: f64) -> R<Node> {
        Ok(Node::F64(v))
    }
    fn serialize_char(self, v: char) -> R<Node> {
        Ok(Node::Str(v.to_string()))
    }
    fn serialize_str(self, v: &str) -> R<Node> {
        Ok(Node::Str(v.to_string()))
    }
    fn serialize_bytes(self, v: &[u8]) -> R<Node> {
        Ok(Node::Bytes(v.to_vec()))
    }
    fn serialize_none(self) -> R<Node> {
        Ok(Node::None)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> R<Node> {
        v.serialize(self)
    }
    fn serialize_unit(self) -> R<Node> {
        Ok(Node::Unit)
    }
    fn serialize_unit_struct(self, _: &'static str) -> R<Node> {
        Ok(Node::Unit)
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, variant: &'static str) -> R<Node> {
        Ok(Node::Str(variant.to_string()))
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, name: &'static str, v: &T) -> R<Node> {
        let n = v.serialize(self)?;
        // Handles are HANDLE tags, which split streams store as differences.
        Ok(match n {
            Node::Int(i) if name == "Handle" && i >= 0 => Node::U64(i.unsigned_abs()),
            n => n,
        })
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(self, _: &'static str, _: u32, variant: &'static str, v: &T) -> R<Node> {
        Ok(Node::Map(vec![(variant.to_string(), v.serialize(self)?)]))
    }
    fn serialize_seq(self, len: Option<usize>) -> R<SeqSer> {
        Ok(SeqSer(Vec::with_capacity(len.unwrap_or(0).min(4096))))
    }
    fn serialize_tuple(self, len: usize) -> R<SeqSer> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(self, _: &'static str, len: usize) -> R<SeqSer> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(self, _: &'static str, _: u32, variant: &'static str, len: usize) -> R<VariantSer<SeqSer>> {
        Ok(VariantSer { variant, inner: self.serialize_seq(Some(len))? })
    }
    fn serialize_map(self, _: Option<usize>) -> R<MapSer> {
        Ok(MapSer { entries: Vec::new(), key: None })
    }
    fn serialize_struct(self, name: &'static str, len: usize) -> R<StructSer> {
        Ok(StructSer { name, fields: Vec::with_capacity(len.min(256)) })
    }
    fn serialize_struct_variant(self, _: &'static str, _: u32, variant: &'static str, len: usize) -> R<VariantSer<StructSer>> {
        Ok(VariantSer { variant, inner: StructSer { name: "", fields: Vec::with_capacity(len.min(256)) } })
    }
    fn serialize_i128(self, v: i128) -> R<Node> {
        i64::try_from(v).map(Node::Int).map_err(|_| NodeError("a 128-bit integer doesn't fit a .cad file".into()))
    }
    fn serialize_u128(self, v: u128) -> R<Node> {
        u64::try_from(v).map_err(|_| NodeError("a 128-bit integer doesn't fit a .cad file".into())).and_then(|v| NodeSer.serialize_u64(v))
    }
}

pub(crate) struct SeqSer(Vec<Node>);

impl ser::SerializeSeq for SeqSer {
    type Ok = Node;
    type Error = NodeError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> R<()> {
        self.0.push(v.serialize(NodeSer)?);
        Ok(())
    }
    fn end(self) -> R<Node> {
        Ok(Node::Seq(self.0))
    }
}
impl ser::SerializeTuple for SeqSer {
    type Ok = Node;
    type Error = NodeError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> R<()> {
        ser::SerializeSeq::serialize_element(self, v)
    }
    fn end(self) -> R<Node> {
        ser::SerializeSeq::end(self)
    }
}
impl ser::SerializeTupleStruct for SeqSer {
    type Ok = Node;
    type Error = NodeError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> R<()> {
        ser::SerializeSeq::serialize_element(self, v)
    }
    fn end(self) -> R<Node> {
        ser::SerializeSeq::end(self)
    }
}

pub(crate) struct MapSer {
    entries: Vec<(Node, Node)>,
    key: Option<Node>,
}

impl ser::SerializeMap for MapSer {
    type Ok = Node;
    type Error = NodeError;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, k: &T) -> R<()> {
        self.key = Some(k.serialize(NodeSer)?);
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> R<()> {
        let k = self.key.take().ok_or_else(|| NodeError("a map value without a key".into()))?;
        self.entries.push((k, v.serialize(NodeSer)?));
        Ok(())
    }
    fn end(self) -> R<Node> {
        if self.entries.iter().all(|(k, _)| matches!(k, Node::Str(_))) {
            Ok(Node::Map(
                self.entries
                    .into_iter()
                    .map(|(k, v)| match k {
                        Node::Str(s) => (s, v),
                        _ => (String::new(), v),
                    })
                    .collect(),
            ))
        } else {
            Ok(Node::MapAny(self.entries))
        }
    }
}

pub(crate) struct StructSer {
    name: &'static str,
    fields: Vec<(String, Node)>,
}

impl ser::SerializeStruct for StructSer {
    type Ok = Node;
    type Error = NodeError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, v: &T) -> R<()> {
        self.fields.push((key.to_string(), v.serialize(NodeSer)?));
        Ok(())
    }
    fn end(self) -> R<Node> {
        let f = &self.fields;
        let named = |i: usize, n: &str| f.get(i).is_some_and(|(k, _)| k == n);
        let real = |i: usize| match f.get(i) {
            Some((_, Node::F64(v))) => Some(*v),
            _ => None,
        };
        // Points and raw DXF groups get tags of their own (docs/cad-format.md §5).
        match (self.name, f.len()) {
            ("Vec2", 2) if named(0, "x") && named(1, "y") => {
                if let (Some(x), Some(y)) = (real(0), real(1)) {
                    return Ok(Node::P2(x, y));
                }
            }
            ("Vec3", 3) if named(0, "x") && named(1, "y") && named(2, "z") => {
                if let (Some(x), Some(y), Some(z)) = (real(0), real(1), real(2)) {
                    return Ok(Node::P3(x, y, z));
                }
            }
            ("RawTag", 2) if named(0, "code") && named(1, "value") => {
                if let (Some((_, Node::Int(c))), Some((_, Node::Str(v)))) = (f.first(), f.get(1))
                    && let Ok(c) = u16::try_from(*c)
                    && c < 2000
                {
                    return Ok(Node::Raw(c, v.clone()));
                }
            }
            _ => {}
        }
        Ok(Node::Map(self.fields))
    }
}

pub(crate) struct VariantSer<S> {
    variant: &'static str,
    inner: S,
}

impl ser::SerializeTupleVariant for VariantSer<SeqSer> {
    type Ok = Node;
    type Error = NodeError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> R<()> {
        ser::SerializeSeq::serialize_element(&mut self.inner, v)
    }
    fn end(self) -> R<Node> {
        Ok(Node::Map(vec![(self.variant.to_string(), ser::SerializeSeq::end(self.inner)?)]))
    }
}
impl ser::SerializeStructVariant for VariantSer<StructSer> {
    type Ok = Node;
    type Error = NodeError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, v: &T) -> R<()> {
        ser::SerializeStruct::serialize_field(&mut self.inner, key, v)
    }
    fn end(self) -> R<Node> {
        Ok(Node::Map(vec![(self.variant.to_string(), Node::Map(self.inner.fields))]))
    }
}

// ---------------------------------------------------------------------------
// Deserialising
// ---------------------------------------------------------------------------

impl<'de> de::Deserializer<'de> for Node {
    type Error = NodeError;

    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> R<V::Value> {
        match self {
            Node::None => v.visit_none(),
            Node::Unit => v.visit_unit(),
            Node::Bool(b) => v.visit_bool(b),
            Node::Int(i) => v.visit_i64(i),
            Node::U64(u) => v.visit_u64(u),
            Node::F64(f) => v.visit_f64(f),
            Node::Str(s) => v.visit_string(s),
            Node::Bytes(b) => v.visit_byte_buf(b),
            Node::P2(x, y) => v.visit_map(MapDe::new(vec![("x".into(), Node::F64(x)), ("y".into(), Node::F64(y))])),
            Node::P3(x, y, z) => v.visit_map(MapDe::new(vec![("x".into(), Node::F64(x)), ("y".into(), Node::F64(y)), ("z".into(), Node::F64(z))])),
            Node::Raw(c, s) => v.visit_map(MapDe::new(vec![("code".into(), Node::Int(c.into())), ("value".into(), Node::Str(s))])),
            Node::Seq(items) => {
                let n = items.len();
                let mut seq = SeqDe(items.into_iter());
                let out = v.visit_seq(&mut seq)?;
                if !seq.0.as_slice().is_empty() {
                    return Err(de::Error::invalid_length(n, &"fewer items"));
                }
                Ok(out)
            }
            Node::Map(entries) => v.visit_map(MapDe::new(entries)),
            Node::MapAny(entries) => v.visit_map(MapAnyDe { it: entries.into_iter(), value: None }),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, v: V) -> R<V::Value> {
        match self {
            Node::None | Node::Unit => v.visit_none(),
            n => v.visit_some(n),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, v: V) -> R<V::Value> {
        match self {
            Node::None | Node::Unit => v.visit_unit(),
            n => n.deserialize_any(v),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _: &'static str, v: V) -> R<V::Value> {
        v.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(self, _: &'static str, _: &'static [&'static str], v: V) -> R<V::Value> {
        match self {
            Node::Str(s) => v.visit_enum(s.into_deserializer()),
            Node::Map(mut entries) if entries.len() == 1 => match entries.pop() {
                Some((variant, value)) => v.visit_enum(EnumDe { variant, value }),
                None => Err(de::Error::custom("an enum without a variant")),
            },
            other => Err(de::Error::custom(format!("expected an enum variant, found {}", kind(&other)))),
        }
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        unit_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}

fn kind(n: &Node) -> &'static str {
    match n {
        Node::None => "none",
        Node::Unit => "unit",
        Node::Bool(_) => "a boolean",
        Node::Int(_) | Node::U64(_) => "an integer",
        Node::F64(_) => "a real",
        Node::Str(_) => "a string",
        Node::Bytes(_) => "bytes",
        Node::P2(..) | Node::P3(..) => "a point",
        Node::Raw(..) => "a DXF group",
        Node::Seq(_) => "a sequence",
        Node::Map(_) | Node::MapAny(_) => "a map",
    }
}

struct SeqDe(std::vec::IntoIter<Node>);

impl<'de> de::SeqAccess<'de> for SeqDe {
    type Error = NodeError;
    fn next_element_seed<T: de::DeserializeSeed<'de>>(&mut self, seed: T) -> R<Option<T::Value>> {
        self.0.next().map(|n| seed.deserialize(n)).transpose()
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}

struct MapDe {
    it: std::vec::IntoIter<(String, Node)>,
    value: Option<Node>,
}

impl MapDe {
    fn new(entries: Vec<(String, Node)>) -> MapDe {
        MapDe { it: entries.into_iter(), value: None }
    }
}

impl<'de> de::MapAccess<'de> for MapDe {
    type Error = NodeError;
    fn next_key_seed<K: de::DeserializeSeed<'de>>(&mut self, seed: K) -> R<Option<K::Value>> {
        let Some((k, v)) = self.it.next() else { return Ok(None) };
        self.value = Some(v);
        seed.deserialize(de::value::StringDeserializer::<NodeError>::new(k)).map(Some)
    }
    fn next_value_seed<V: de::DeserializeSeed<'de>>(&mut self, seed: V) -> R<V::Value> {
        seed.deserialize(self.value.take().unwrap_or(Node::None))
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.it.len())
    }
}

struct MapAnyDe {
    it: std::vec::IntoIter<(Node, Node)>,
    value: Option<Node>,
}

impl<'de> de::MapAccess<'de> for MapAnyDe {
    type Error = NodeError;
    fn next_key_seed<K: de::DeserializeSeed<'de>>(&mut self, seed: K) -> R<Option<K::Value>> {
        let Some((k, v)) = self.it.next() else { return Ok(None) };
        self.value = Some(v);
        seed.deserialize(k).map(Some)
    }
    fn next_value_seed<V: de::DeserializeSeed<'de>>(&mut self, seed: V) -> R<V::Value> {
        seed.deserialize(self.value.take().unwrap_or(Node::None))
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.it.len())
    }
}

struct EnumDe {
    variant: String,
    value: Node,
}

impl<'de> de::EnumAccess<'de> for EnumDe {
    type Error = NodeError;
    type Variant = Node;
    fn variant_seed<V: de::DeserializeSeed<'de>>(self, seed: V) -> R<(V::Value, Node)> {
        let v = seed.deserialize(de::value::StringDeserializer::<NodeError>::new(self.variant))?;
        Ok((v, self.value))
    }
}

impl<'de> de::VariantAccess<'de> for Node {
    type Error = NodeError;
    fn unit_variant(self) -> R<()> {
        Ok(())
    }
    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> R<T::Value> {
        seed.deserialize(self)
    }
    fn tuple_variant<V: Visitor<'de>>(self, _: usize, v: V) -> R<V::Value> {
        de::Deserializer::deserialize_any(self, v)
    }
    fn struct_variant<V: Visitor<'de>>(self, _: &'static [&'static str], v: V) -> R<V::Value> {
        de::Deserializer::deserialize_any(self, v)
    }
}
