//! Serde adapters walk the archive directly: no intermediate JSON text or tree.
use super::{ArchivedNode, MAX_DEPTH, Node};
use serde::{Deserialize, Serialize, de, ser};
use std::fmt;

#[derive(Debug)]
pub(super) struct Error(String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}
impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}
type Result<T> = std::result::Result<T, Error>;

pub(super) struct Encoder {
    pub nodes: Vec<Node>,
    depth: usize,
}
impl Encoder {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            depth: 0,
        }
    }
    fn container(&mut self, object: bool) -> Result<Container<'_>> {
        if self.depth >= MAX_DEPTH {
            return Err(Error("远端数据嵌套过深".into()));
        }
        self.depth += 1;
        let index = self.nodes.len();
        self.nodes.push(if object {
            Node::Object(0)
        } else {
            Node::Array(0)
        });
        Ok(Container {
            encoder: self,
            index,
            count: 0,
            levels: 1,
        })
    }
}
pub(super) struct Container<'a> {
    encoder: &'a mut Encoder,
    index: usize,
    count: u32,
    levels: usize,
}
impl Container<'_> {
    fn element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut *self.encoder)?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or_else(|| Error("远端容器过大".into()))?;
        Ok(())
    }
    fn field<T: Serialize + ?Sized>(&mut self, key: &str, value: &T) -> Result<()> {
        self.encoder.nodes.push(Node::Text(key.to_owned()));
        self.element(value)
    }
    fn finish(self) -> Result<()> {
        match &mut self.encoder.nodes[self.index] {
            Node::Array(count) | Node::Object(count) => *count = self.count,
            _ => unreachable!(),
        }
        self.encoder.depth -= self.levels;
        Ok(())
    }
}
impl<'a> ser::Serializer for &'a mut Encoder {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Container<'a>;
    type SerializeTuple = Container<'a>;
    type SerializeTupleStruct = Container<'a>;
    type SerializeTupleVariant = Container<'a>;
    type SerializeMap = Container<'a>;
    type SerializeStruct = Container<'a>;
    type SerializeStructVariant = Container<'a>;
    fn serialize_bool(self, v: bool) -> Result<()> {
        self.nodes.push(Node::Bool(v));
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> Result<()> {
        self.serialize_i64(v.into())
    }
    fn serialize_i16(self, v: i16) -> Result<()> {
        self.serialize_i64(v.into())
    }
    fn serialize_i32(self, v: i32) -> Result<()> {
        self.serialize_i64(v.into())
    }
    fn serialize_i64(self, v: i64) -> Result<()> {
        self.nodes.push(Node::Signed(v));
        Ok(())
    }
    fn serialize_u8(self, v: u8) -> Result<()> {
        self.serialize_u64(v.into())
    }
    fn serialize_u16(self, v: u16) -> Result<()> {
        self.serialize_u64(v.into())
    }
    fn serialize_u32(self, v: u32) -> Result<()> {
        self.serialize_u64(v.into())
    }
    fn serialize_u64(self, v: u64) -> Result<()> {
        self.nodes.push(Node::Unsigned(v));
        Ok(())
    }
    fn serialize_f32(self, v: f32) -> Result<()> {
        self.serialize_f64(v.into())
    }
    fn serialize_f64(self, v: f64) -> Result<()> {
        if !v.is_finite() {
            return Err(Error("无效数字".into()));
        }
        self.nodes.push(Node::Float(v));
        Ok(())
    }
    fn serialize_char(self, v: char) -> Result<()> {
        self.serialize_str(&v.to_string())
    }
    fn serialize_str(self, v: &str) -> Result<()> {
        self.nodes.push(Node::Text(v.into()));
        Ok(())
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        self.nodes.push(Node::Bytes(v.to_vec()));
        Ok(())
    }
    fn serialize_none(self) -> Result<()> {
        self.serialize_unit()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<()> {
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<()> {
        self.nodes.push(Node::Null);
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<()> {
        self.serialize_unit()
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, variant: &'static str) -> Result<()> {
        self.serialize_str(variant)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, v: &T) -> Result<()> {
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<()> {
        let mut map = self.container(true)?;
        map.field(variant, v)?;
        map.finish()
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Container<'a>> {
        self.container(false)
    }
    fn serialize_tuple(self, _: usize) -> Result<Container<'a>> {
        self.container(false)
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Container<'a>> {
        self.container(false)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Container<'a>> {
        let outer = self.container(true)?;
        outer.encoder.nodes[outer.index] = Node::Object(1);
        outer.encoder.nodes.push(Node::Text(variant.into()));
        let mut inner = outer.encoder.container(false)?;
        inner.levels = 2;
        Ok(inner)
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Container<'a>> {
        self.container(true)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Container<'a>> {
        self.container(true)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Container<'a>> {
        let outer = self.container(true)?;
        outer.encoder.nodes[outer.index] = Node::Object(1);
        outer.encoder.nodes.push(Node::Text(variant.into()));
        let mut inner = outer.encoder.container(true)?;
        inner.levels = 2;
        Ok(inner)
    }
    fn is_human_readable(&self) -> bool {
        true
    }
}
macro_rules! sequence {
    ($trait:ident, $method:ident) => {
        impl ser::$trait for Container<'_> {
            type Ok = ();
            type Error = Error;
            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
                self.element(value)
            }
            fn end(self) -> Result<()> {
                self.finish()
            }
        }
    };
}
sequence!(SerializeSeq, serialize_element);
sequence!(SerializeTuple, serialize_element);
sequence!(SerializeTupleStruct, serialize_field);
sequence!(SerializeTupleVariant, serialize_field);
impl ser::SerializeMap for Container<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<()> {
        // Preserve Serde JSON's string-key convention for existing domain maps.
        let key = serde_json::to_value(key).map_err(|e| Error(e.to_string()))?;
        let text = match key {
            serde_json::Value::String(v) => v,
            serde_json::Value::Number(v) => v.to_string(),
            serde_json::Value::Bool(v) => v.to_string(),
            _ => return Err(Error("无效对象键".into())),
        };
        self.encoder.nodes.push(Node::Text(text));
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        self.element(value)
    }
    fn end(self) -> Result<()> {
        self.finish()
    }
}
impl ser::SerializeStruct for Container<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.field(key, value)
    }
    fn end(self) -> Result<()> {
        self.finish()
    }
}
impl ser::SerializeStructVariant for Container<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.field(key, value)
    }
    fn end(self) -> Result<()> {
        self.finish()
    }
}

pub(super) struct Decoder<'a> {
    nodes: &'a rkyv::vec::ArchivedVec<ArchivedNode>,
    pub at: usize,
    depth: usize,
}
impl<'a> Decoder<'a> {
    pub fn new(nodes: &'a rkyv::vec::ArchivedVec<ArchivedNode>) -> Self {
        Self {
            nodes,
            at: 0,
            depth: 0,
        }
    }
    fn next(&mut self) -> Result<&'a ArchivedNode> {
        let node = self
            .nodes
            .get(self.at)
            .ok_or_else(|| Error("远端数据不完整".into()))?;
        self.at += 1;
        Ok(node)
    }
}
struct Access<'a, 'b> {
    decoder: &'b mut Decoder<'a>,
    left: usize,
}
impl<'de> de::SeqAccess<'de> for Access<'de, '_> {
    type Error = Error;
    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>> {
        if self.left == 0 {
            return Ok(None);
        }
        self.left -= 1;
        seed.deserialize(&mut *self.decoder).map(Some)
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.left)
    }
}
impl<'de> de::MapAccess<'de> for Access<'de, '_> {
    type Error = Error;
    fn next_key_seed<T: de::DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        if self.left == 0 {
            return Ok(None);
        }
        if !matches!(
            self.decoder.nodes.get(self.decoder.at),
            Some(ArchivedNode::Text(_))
        ) {
            return Err(Error("无效对象键".into()));
        }
        seed.deserialize(&mut *self.decoder).map(Some)
    }
    fn next_value_seed<T: de::DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Value> {
        self.left -= 1;
        seed.deserialize(&mut *self.decoder)
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.left)
    }
}
impl<'de> de::EnumAccess<'de> for &mut Decoder<'de> {
    type Error = Error;
    type Variant = Self;
    fn variant_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<(T::Value, Self)> {
        let variant = seed.deserialize(&mut *self)?;
        Ok((variant, self))
    }
}
impl<'de> de::VariantAccess<'de> for &mut Decoder<'de> {
    type Error = Error;
    fn unit_variant(self) -> Result<()> {
        <()>::deserialize(self)
    }
    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        seed.deserialize(self)
    }
    fn tuple_variant<V: de::Visitor<'de>>(self, _: usize, visitor: V) -> Result<V::Value> {
        de::Deserializer::deserialize_any(self, visitor)
    }
    fn struct_variant<V: de::Visitor<'de>>(
        self,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        de::Deserializer::deserialize_any(self, visitor)
    }
}
impl<'de> de::Deserializer<'de> for &mut Decoder<'de> {
    type Error = Error;
    fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if self.depth >= MAX_DEPTH {
            return Err(Error("远端数据嵌套过深".into()));
        }
        match self.next()? {
            ArchivedNode::Null => visitor.visit_unit(),
            ArchivedNode::Bool(v) => visitor.visit_bool(*v),
            ArchivedNode::Signed(v) => visitor.visit_i64(v.to_native()),
            ArchivedNode::Unsigned(v) => visitor.visit_u64(v.to_native()),
            ArchivedNode::Float(v) => {
                let v = v.to_native();
                if !v.is_finite() {
                    return Err(Error("无效数字".into()));
                }
                visitor.visit_f64(v)
            }
            ArchivedNode::Text(v) => visitor.visit_borrowed_str(v.as_str()),
            ArchivedNode::Bytes(v) => visitor.visit_borrowed_bytes(v.as_slice()),
            node @ (ArchivedNode::Array(_) | ArchivedNode::Object(_)) => {
                let (count, object) = match node {
                    ArchivedNode::Array(n) => (n.to_native() as usize, false),
                    ArchivedNode::Object(n) => (n.to_native() as usize, true),
                    _ => unreachable!(),
                };
                if count > self.nodes.len().saturating_sub(self.at) / if object { 2 } else { 1 } {
                    return Err(Error("无效容器长度".into()));
                }
                self.depth += 1;
                let mut access = Access {
                    decoder: self,
                    left: count,
                };
                let result = if object {
                    visitor.visit_map(&mut access)
                } else {
                    visitor.visit_seq(&mut access)
                };
                let left = access.left;
                self.depth -= 1;
                if left != 0 {
                    return Err(Error("远端容器未完整消费".into()));
                }
                result
            }
        }
    }
    fn deserialize_option<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        if matches!(self.nodes.get(self.at), Some(ArchivedNode::Null)) {
            self.at += 1;
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }
    fn deserialize_newtype_struct<V: de::Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        match self.next()? {
            ArchivedNode::Text(s) => {
                visitor.visit_enum(de::value::BorrowedStrDeserializer::<Error>::new(s.as_str()))
            }
            ArchivedNode::Object(n) if n.to_native() == 1 => visitor.visit_enum(self),
            _ => Err(Error("无效枚举".into())),
        }
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        unit unit_struct seq tuple tuple_struct map struct identifier ignored_any
    }
    fn is_human_readable(&self) -> bool {
        true
    }
}
