//! Typed values and their wire encoding.

/// A D-Bus value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `y`
    Byte(u8),
    /// `b`
    Bool(bool),
    /// `i`
    I32(i32),
    /// `u`
    U32(u32),
    /// `x`
    I64(i64),
    /// `t`
    U64(u64),
    /// `d`
    F64(f64),
    /// `s`
    Str(String),
    /// `o`
    Path(String),
    /// `g`
    Signature(String),
    /// `a…`: the element signature, which an empty array still needs, and
    /// the elements, each of that signature.
    Array(String, Vec<Value>),
    /// `(…)`
    Struct(Vec<Value>),
    /// `{kv}`, only as an array element.
    Entry(Box<(Value, Value)>),
    /// `v`
    Variant(Box<Value>),
}

/// Containers nest at most this deep, as the specification allows.
const DEPTH: usize = 64;

impl Value {
    /// A string.
    pub fn str(text: impl Into<String>) -> Self {
        Self::Str(text.into())
    }

    /// A variant holding `value`.
    pub fn variant(value: Value) -> Self {
        Self::Variant(Box::new(value))
    }

    /// An `a{sv}` dictionary.
    pub fn dict<'a>(entries: impl IntoIterator<Item = (&'a str, Value)>) -> Self {
        let entries = entries
            .into_iter()
            .map(|(key, value)| Self::Entry(Box::new((Self::str(key), Self::variant(value)))))
            .collect();
        Self::Array("{sv}".into(), entries)
    }

    /// An `as` array.
    pub fn strings<'a>(items: impl IntoIterator<Item = &'a str>) -> Self {
        Self::Array("s".into(), items.into_iter().map(Self::str).collect())
    }

    /// Its signature, appended to `out`.
    pub fn signature(&self, out: &mut String) {
        let code = match self {
            Self::Byte(_) => 'y',
            Self::Bool(_) => 'b',
            Self::I32(_) => 'i',
            Self::U32(_) => 'u',
            Self::I64(_) => 'x',
            Self::U64(_) => 't',
            Self::F64(_) => 'd',
            Self::Str(_) => 's',
            Self::Path(_) => 'o',
            Self::Signature(_) => 'g',
            Self::Variant(_) => 'v',
            Self::Array(element, _) => {
                out.push('a');
                out.push_str(element);
                return;
            }
            Self::Struct(fields) => {
                out.push('(');
                fields.iter().for_each(|field| field.signature(out));
                out.push(')');
                return;
            }
            Self::Entry(entry) => {
                out.push('{');
                entry.0.signature(out);
                entry.1.signature(out);
                out.push('}');
                return;
            }
        };
        out.push(code);
    }

    /// A string, object path or signature's text, looking through a variant.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(text) | Self::Path(text) | Self::Signature(text) => Some(text),
            Self::Variant(inner) => inner.as_str(),
            _ => None,
        }
    }

    /// An unsigned or nonnegative integer, looking through a variant.
    pub fn as_u64(&self) -> Option<u64> {
        match *self {
            Self::Byte(n) => Some(n.into()),
            Self::U32(n) => Some(n.into()),
            Self::U64(n) => Some(n),
            Self::I32(n) => n.try_into().ok(),
            Self::I64(n) => n.try_into().ok(),
            Self::Variant(ref inner) => inner.as_u64(),
            _ => None,
        }
    }

    /// A signed integer, looking through a variant.
    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Self::I32(n) => Some(n.into()),
            Self::I64(n) => Some(n),
            Self::Byte(_) | Self::U32(_) | Self::U64(_) => self.as_u64()?.try_into().ok(),
            Self::Variant(ref inner) => inner.as_i64(),
            _ => None,
        }
    }

    /// A number of any numeric type, looking through a variant.
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Self::F64(n) => Some(n),
            Self::Variant(ref inner) => inner.as_f64(),
            _ => self.as_i64().map(|n| n as f64),
        }
    }

    /// A boolean, looking through a variant.
    pub fn as_bool(&self) -> Option<bool> {
        match *self {
            Self::Bool(b) => Some(b),
            Self::Variant(ref inner) => inner.as_bool(),
            _ => None,
        }
    }

    /// The items of an array or the fields of a struct, looking through a
    /// variant.
    pub fn items(&self) -> &[Value] {
        match self {
            Self::Array(_, items) | Self::Struct(items) => items,
            Self::Variant(inner) => inner.items(),
            _ => &[],
        }
    }

    /// The value under `key` in a dictionary with string keys, without its
    /// variant wrapper.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.items().iter().find_map(|item| match item {
            Self::Entry(entry) if entry.0.as_str() == Some(key) => match &entry.1 {
                Self::Variant(inner) => Some(&**inner),
                value => Some(value),
            },
            _ => None,
        })
    }
}

fn alignment(code: u8) -> usize {
    match code {
        b'y' | b'g' | b'v' => 1,
        b'n' | b'q' => 2,
        b'x' | b't' | b'd' | b'(' | b'{' => 8,
        _ => 4,
    }
}

/// Appends values in wire order; alignment is relative to the start of `out`,
/// which must itself start 8-aligned in the message.
pub(crate) fn write(out: &mut Vec<u8>, value: &Value) {
    let pad = |out: &mut Vec<u8>, to: usize| out.resize(out.len().next_multiple_of(to), 0);
    match value {
        Value::Byte(n) => out.push(*n),
        Value::Bool(b) => {
            pad(out, 4);
            out.extend(u32::from(*b).to_le_bytes());
        }
        Value::I32(n) => {
            pad(out, 4);
            out.extend(n.to_le_bytes());
        }
        Value::U32(n) => {
            pad(out, 4);
            out.extend(n.to_le_bytes());
        }
        Value::I64(n) => {
            pad(out, 8);
            out.extend(n.to_le_bytes());
        }
        Value::U64(n) => {
            pad(out, 8);
            out.extend(n.to_le_bytes());
        }
        Value::F64(n) => {
            pad(out, 8);
            out.extend(n.to_le_bytes());
        }
        Value::Str(text) | Value::Path(text) => {
            pad(out, 4);
            out.extend((text.len() as u32).to_le_bytes());
            out.extend(text.as_bytes());
            out.push(0);
        }
        Value::Signature(text) => {
            out.push(text.len() as u8);
            out.extend(text.as_bytes());
            out.push(0);
        }
        Value::Array(element, items) => {
            pad(out, 4);
            let length = out.len();
            out.extend([0; 4]);
            pad(out, alignment(element.as_bytes()[0]));
            let start = out.len();
            items.iter().for_each(|item| write(out, item));
            let bytes = (out.len() - start) as u32;
            out[length..length + 4].copy_from_slice(&bytes.to_le_bytes());
        }
        Value::Struct(fields) => {
            pad(out, 8);
            fields.iter().for_each(|field| write(out, field));
        }
        Value::Entry(entry) => {
            pad(out, 8);
            write(out, &entry.0);
            write(out, &entry.1);
        }
        Value::Variant(inner) => {
            let mut signature = String::new();
            inner.signature(&mut signature);
            write(out, &Value::Signature(signature));
            write(out, inner);
        }
    }
}

/// The first complete type of a signature and the rest, or `None` if it is
/// malformed.
pub(crate) fn split(signature: &str) -> Option<(&str, &str)> {
    let bytes = signature.as_bytes();
    let mut end = 0;
    // Each `a` prefixes the type after it.
    while bytes.get(end) == Some(&b'a') {
        end += 1;
    }
    let close = match *bytes.get(end)? {
        b'(' => b')',
        b'{' => b'}',
        b')' | b'}' => return None,
        _ => return Some(signature.split_at(end + 1)),
    };
    let mut depth = 0usize;
    for (at, &byte) in bytes.iter().enumerate().skip(end) {
        if byte == bytes[end] {
            depth += 1;
        } else if byte == close {
            depth -= 1;
            if depth == 0 {
                return Some(signature.split_at(at + 1));
            }
        }
    }
    None
}

/// Bounds-checked decoder; alignment is relative to the message start.
pub(crate) struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    pub big: bool,
}

impl<'a> Reader<'a> {
    pub fn align(&mut self, to: usize) -> Option<()> {
        self.pos = self.pos.next_multiple_of(to);
        (self.pos <= self.data.len()).then_some(())
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let bytes = self.data.get(self.pos..self.pos.checked_add(len)?)?;
        self.pos += len;
        Some(bytes)
    }

    fn fixed<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.align(N)?;
        let mut bytes: [u8; N] = self.take(N)?.try_into().ok()?;
        if self.big {
            bytes.reverse();
        }
        Some(bytes)
    }

    pub fn u32(&mut self) -> Option<u32> {
        self.fixed().map(u32::from_le_bytes)
    }

    fn text(&mut self, len: usize) -> Option<String> {
        let text = std::str::from_utf8(self.take(len)?).ok()?.to_owned();
        (self.take(1)? == [0]).then_some(text)
    }

    /// One value of the single complete type `signature`.
    pub fn value(&mut self, signature: &str, depth: usize) -> Option<Value> {
        if depth > DEPTH {
            return None;
        }
        Some(match signature.as_bytes()[0] {
            b'y' => Value::Byte(self.take(1)?[0]),
            b'b' => Value::Bool(match self.u32()? {
                0 => false,
                1 => true,
                _ => return None,
            }),
            b'i' => Value::I32(i32::from_le_bytes(self.fixed()?)),
            b'u' => Value::U32(self.u32()?),
            b'x' => Value::I64(i64::from_le_bytes(self.fixed()?)),
            b't' => Value::U64(u64::from_le_bytes(self.fixed()?)),
            b'd' => Value::F64(f64::from_le_bytes(self.fixed()?)),
            b's' | b'o' => {
                let len = self.u32()? as usize;
                let text = self.text(len)?;
                match signature.as_bytes()[0] {
                    b's' => Value::Str(text),
                    _ => Value::Path(text),
                }
            }
            b'g' => {
                let len = self.take(1)?[0] as usize;
                Value::Signature(self.text(len)?)
            }
            b'v' => {
                let len = self.take(1)?[0] as usize;
                let inner = self.text(len)?;
                let (single, "") = split(&inner)? else {
                    return None;
                };
                Value::variant(self.value(single, depth + 1)?)
            }
            b'a' => {
                let element = &signature[1..];
                let len = self.u32()? as usize;
                self.align(alignment(element.as_bytes()[0]))?;
                let end = self.pos.checked_add(len)?;
                if end > self.data.len() {
                    return None;
                }
                let mut items = Vec::new();
                while self.pos < end {
                    items.push(self.value(element, depth + 1)?);
                }
                (self.pos == end).then_some(())?;
                Value::Array(element.into(), items)
            }
            b'(' => {
                self.align(8)?;
                let mut fields = Vec::new();
                let mut rest = &signature[1..signature.len() - 1];
                while !rest.is_empty() {
                    let (field, tail) = split(rest)?;
                    fields.push(self.value(field, depth + 1)?);
                    rest = tail;
                }
                Value::Struct(fields)
            }
            b'{' => {
                self.align(8)?;
                let (key, value) = split(&signature[1..signature.len() - 1])?;
                let entry = (self.value(key, depth + 1)?, self.value(value, depth + 1)?);
                Value::Entry(Box::new(entry))
            }
            _ => return None,
        })
    }
}
