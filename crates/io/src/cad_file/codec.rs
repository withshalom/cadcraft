//! Nodes as tags (docs/cad-format.md §4–§5): named values under the codes of the file's name
//! dictionary, positional values, compound forms and end tags.

use std::collections::HashMap;

use cad_native::cad_model::geom::{Vec2, Vec3};
use cad_native::cad_model::model::Handle as TagHandle;
use cad_native::tags::{Tag, TagWriter, Value, read_tag};

use super::node::Node;

/// Envelope: the record kind (first tag).
pub(crate) const CODE_KIND: u16 = 0;
/// Envelope: the entity's handle.
pub(crate) const CODE_HANDLE: u16 = 5;
/// Envelope: the layout whose paper space holds the entity.
pub(crate) const CODE_LAYOUT: u16 = 410;
/// Envelope: the block whose definition holds the entity.
pub(crate) const CODE_BLOCK: u16 = 2001;
/// A scalar value without a name.
const POS: u16 = 2010;
/// A compound value without a name.
const POS_COMPOUND: u16 = 2011;
/// End of the innermost sequence or map.
const END: u16 = 2012;
/// Dictionary entry `i`: a scalar under `NAMED + 2i`, a compound under `NAMED + 2i + 1`.
const NAMED: u16 = 3000;
/// Most names a file's dictionary holds.
pub(crate) const MAX_NAMES: usize = 31_000;
/// Names kept free for record fields: a map of user keys that would eat into them is written with
/// positional keys instead.
const RESERVED_NAMES: usize = 1_000;
/// Deepest nesting of values.
pub(crate) const MAX_DEPTH: usize = 64;

const FORM_SEQ: i16 = 1;
const FORM_MAP: i16 = 2;
const FORM_MAP_ANY: i16 = 3;
const FORM_NONE: i16 = 4;
const FORM_UNIT: i16 = 5;

/// The file's name dictionary as it is written: names in order of first use.
#[derive(Default)]
pub(crate) struct Dict {
    pub(crate) names: Vec<String>,
    index: HashMap<String, u16>,
}

impl Dict {
    /// The scalar code of `name`, adding it when there is room for `limit` names.
    fn code_within(&mut self, name: &str, limit: usize) -> Option<u16> {
        if let Some(&c) = self.index.get(name) {
            return Some(c);
        }
        if self.names.len() >= limit {
            return None;
        }
        let c = u16::try_from(self.names.len()).ok()?.checked_mul(2)?.checked_add(NAMED)?;
        self.names.push(name.to_string());
        self.index.insert(name.to_string(), c);
        Some(c)
    }
    fn missing(&self, keys: &[(String, Node)]) -> usize {
        keys.iter().filter(|(k, _)| !self.index.contains_key(k)).count()
    }
}

#[derive(Clone, Copy)]
enum Label {
    Named(u16),
    Pos,
}

/// Write a record body: each field under its name's code (no end tag).
pub(crate) fn put_body(w: &mut TagWriter, fields: &[(String, Node)], dict: &mut Dict) -> Result<(), String> {
    for (k, v) in fields {
        let c = dict.code_within(k, MAX_NAMES).ok_or("the drawing has more field names than a .cad file holds")?;
        put(w, Label::Named(c), v, dict, 1)?;
    }
    Ok(())
}

/// Write a positional value (a header variable).
pub(crate) fn put_positional(w: &mut TagWriter, v: &Node, dict: &mut Dict) -> Result<(), String> {
    put(w, Label::Pos, v, dict, 1)
}

fn put(w: &mut TagWriter, label: Label, n: &Node, dict: &mut Dict, depth: usize) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!("values nest deeper than {MAX_DEPTH} levels"));
    }
    let (s, c) = match label {
        Label::Named(code) => (code, code.saturating_add(1)),
        Label::Pos => (POS, POS_COMPOUND),
    };
    match n {
        Node::None => w.i16(c, FORM_NONE),
        Node::Unit => w.i16(c, FORM_UNIT),
        Node::Bool(b) => w.bool(s, *b),
        Node::Int(i) => match (i16::try_from(*i), i32::try_from(*i)) {
            (Ok(v), _) => w.i16(s, v),
            (_, Ok(v)) => w.i32(s, v),
            _ => w.i64(s, *i),
        },
        Node::U64(u) => w.handle(s, TagHandle(*u)),
        Node::F64(f) => w.f64(s, *f),
        Node::Str(t) => w.str(s, t),
        Node::Bytes(b) => w.bytes(s, b),
        Node::P2(x, y) => w.p2(s, Vec2::new(*x, *y)),
        Node::P3(x, y, z) => w.p3(s, Vec3::new(*x, *y, *z)),
        Node::Raw(code, v) => match label {
            Label::Pos => w.str(*code, v),
            Label::Named(_) => {
                let m = Node::Map(vec![("code".into(), Node::Int((*code).into())), ("value".into(), Node::Str(v.clone()))]);
                return put(w, label, &m, dict, depth);
            }
        },
        Node::Seq(items) => {
            w.i16(c, FORM_SEQ);
            for it in items {
                put(w, Label::Pos, it, dict, depth + 1)?;
            }
            w.bool(END, true);
        }
        Node::Map(entries) => {
            if dict.names.len() + dict.missing(entries) <= MAX_NAMES - RESERVED_NAMES {
                w.i16(c, FORM_MAP);
                for (k, v) in entries {
                    let code = dict.code_within(k, MAX_NAMES).ok_or("the name dictionary is full")?;
                    put(w, Label::Named(code), v, dict, depth + 1)?;
                }
            } else {
                w.i16(c, FORM_MAP_ANY);
                for (k, v) in entries {
                    w.str(POS, k);
                    put(w, Label::Pos, v, dict, depth + 1)?;
                }
            }
            w.bool(END, true);
        }
        Node::MapAny(entries) => {
            w.i16(c, FORM_MAP_ANY);
            for (k, v) in entries {
                put(w, Label::Pos, k, dict, depth + 1)?;
                put(w, Label::Pos, v, dict, depth + 1)?;
            }
            w.bool(END, true);
        }
    }
    Ok(())
}

/// Every tag of a record or tag stream.
pub(crate) fn read_tags(data: &[u8]) -> Result<Vec<Tag>, String> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < data.len() {
        out.push(read_tag(data, &mut pos).map_err(|e| e.to_string())?);
    }
    Ok(out)
}

fn is_compound(code: u16) -> bool {
    code == POS_COMPOUND || (code >= NAMED && (code - NAMED) % 2 == 1)
}

fn name(code: u16, names: &[String]) -> Result<&str, String> {
    if code < NAMED {
        return Err(format!("group {code} where a named value belongs"));
    }
    names.get(usize::from((code - NAMED) / 2)).map(String::as_str).ok_or_else(|| format!("group {code} names no entry of the dictionary"))
}

fn scalar(v: &Value) -> Option<Node> {
    Some(match v {
        Value::Str(s) => Node::Str(s.clone()),
        Value::F64(f) => Node::F64(*f),
        Value::I16(i) => Node::Int((*i).into()),
        Value::I32(i) => Node::Int((*i).into()),
        Value::I64(i) => Node::Int(*i),
        Value::Bool(b) => Node::Bool(*b),
        Value::Handle(h) => Node::U64(*h),
        Value::Bytes(b) => Node::Bytes(b.clone()),
        Value::P2(p) => Node::P2(p.x, p.y),
        Value::P3(p) => Node::P3(p.x, p.y, p.z),
        // A value type of a newer revision: skipped.
        Value::Unknown { .. } => return None,
    })
}

/// Read the value starting at `tags[*i]`. `None`: a value of a newer type, skipped.
pub(crate) fn value(tags: &[Tag], i: &mut usize, names: &[String], depth: usize) -> Result<Option<Node>, String> {
    if depth > MAX_DEPTH {
        return Err(format!("values nest deeper than {MAX_DEPTH} levels"));
    }
    let t = tags.get(*i).ok_or("a value is missing at the end of the record")?;
    *i += 1;
    if t.code == END {
        return Err("an end tag without a sequence or map".into());
    }
    if !is_compound(t.code) {
        return Ok(scalar(&t.value));
    }
    let Value::I16(form) = t.value else { return Err(format!("group {}: a compound value without its form", t.code)) };
    let at_end = |i: &mut usize| -> Result<bool, String> {
        match tags.get(*i) {
            None => Err("a sequence or map runs past the end of the record".into()),
            Some(t) if t.code == END => {
                *i += 1;
                Ok(true)
            }
            Some(_) => Ok(false),
        }
    };
    Ok(Some(match form {
        FORM_NONE => Node::None,
        FORM_UNIT => Node::Unit,
        FORM_SEQ => {
            let mut items = Vec::new();
            while !at_end(i)? {
                match tags.get(*i) {
                    // A raw DXF group: its own code and its value text.
                    Some(t) if t.code < 2000 => {
                        *i += 1;
                        if let Value::Str(s) = &t.value {
                            items.push(Node::Raw(t.code, s.clone()));
                        }
                    }
                    _ => {
                        if let Some(n) = value(tags, i, names, depth + 1)? {
                            items.push(n);
                        }
                    }
                }
            }
            Node::Seq(items)
        }
        FORM_MAP => {
            let mut entries = Vec::new();
            while !at_end(i)? {
                let code = tags.get(*i).map_or(0, |t| t.code);
                let k = name(code, names)?.to_string();
                if let Some(v) = value(tags, i, names, depth + 1)? {
                    entries.push((k, v));
                }
            }
            Node::Map(entries)
        }
        FORM_MAP_ANY => {
            let mut entries = Vec::new();
            while !at_end(i)? {
                let k = value(tags, i, names, depth + 1)?;
                let v = value(tags, i, names, depth + 1)?;
                if let (Some(k), Some(v)) = (k, v) {
                    entries.push((k, v));
                }
            }
            // String keys read as a struct or a string-keyed map, as they were.
            if entries.iter().all(|(k, _)| matches!(k, Node::Str(_))) {
                Node::Map(
                    entries
                        .into_iter()
                        .map(|(k, v)| match k {
                            Node::Str(s) => (s, v),
                            _ => (String::new(), v),
                        })
                        .collect(),
                )
            } else {
                Node::MapAny(entries)
            }
        }
        f => return Err(format!("group {}: unknown value form {f}", t.code)),
    }))
}

/// A record body: named values from `tags[*i]` to the end.
pub(crate) fn body(tags: &[Tag], i: &mut usize, names: &[String]) -> Result<Vec<(String, Node)>, String> {
    let mut out = Vec::new();
    while let Some(t) = tags.get(*i) {
        let k = name(t.code, names)?.to_string();
        if let Some(v) = value(tags, i, names, 1)? {
            out.push((k, v));
        }
    }
    Ok(out)
}
