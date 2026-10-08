//! Messages: their header fields and body, and the wire framing.

use crate::{
    MESSAGE_LIMIT, Value,
    value::{Reader, split, write},
};

/// What a message is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A method call.
    Call = 1,
    /// A method's return.
    Return = 2,
    /// A method's error.
    Error = 3,
    /// A signal.
    Signal = 4,
}

/// A message. Empty strings stand for absent header fields and `0` for an
/// absent reply serial.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    /// Its kind.
    pub kind: Kind,
    /// Assigned by [`crate::Connection::send`]; set on received messages.
    pub serial: u32,
    /// The call a return or error answers.
    pub reply_serial: u32,
    /// The object path of a call or signal.
    pub path: String,
    /// The interface of a call or signal.
    pub interface: String,
    /// The method or signal name.
    pub member: String,
    /// The name of an error.
    pub error: String,
    /// The bus name it goes to.
    pub destination: String,
    /// The unique bus name it came from, set by the bus.
    pub sender: String,
    /// Whether a call wants no reply.
    pub no_reply: bool,
    /// Its arguments.
    pub body: Vec<Value>,
}

impl Message {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            serial: 0,
            reply_serial: 0,
            path: String::new(),
            interface: String::new(),
            member: String::new(),
            error: String::new(),
            destination: String::new(),
            sender: String::new(),
            no_reply: false,
            body: Vec::new(),
        }
    }

    /// A method call.
    pub fn call(
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        body: Vec<Value>,
    ) -> Self {
        Self {
            destination: destination.into(),
            path: path.into(),
            interface: interface.into(),
            member: member.into(),
            body,
            ..Self::new(Kind::Call)
        }
    }

    /// A signal from the object at `path`.
    pub fn signal(path: &str, interface: &str, member: &str, body: Vec<Value>) -> Self {
        Self {
            path: path.into(),
            interface: interface.into(),
            member: member.into(),
            body,
            ..Self::new(Kind::Signal)
        }
    }

    /// The return of this call.
    pub fn reply(&self, body: Vec<Value>) -> Self {
        Self {
            reply_serial: self.serial,
            destination: self.sender.clone(),
            body,
            ..Self::new(Kind::Return)
        }
    }

    /// An error answering this call.
    pub fn error(&self, name: &str, text: &str) -> Self {
        Self {
            reply_serial: self.serial,
            destination: self.sender.clone(),
            error: name.into(),
            body: vec![Value::str(text)],
            ..Self::new(Kind::Error)
        }
    }

    /// Whether this is a call of `interface.member`.
    pub fn is_call(&self, interface: &str, member: &str) -> bool {
        self.kind == Kind::Call && self.interface == interface && self.member == member
    }

    /// Whether this is the signal `interface.member`.
    pub fn is_signal(&self, interface: &str, member: &str) -> bool {
        self.kind == Kind::Signal && self.interface == interface && self.member == member
    }

    /// The little-endian wire form with `serial`.
    pub(crate) fn encode(&self, serial: u32) -> Vec<u8> {
        let mut body = Vec::new();
        let mut signature = String::new();
        for value in &self.body {
            write(&mut body, value);
            value.signature(&mut signature);
        }
        let flags = u8::from(self.no_reply);
        let mut out = vec![b'l', self.kind as u8, flags, 1];
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(serial.to_le_bytes());
        let mut fields = Vec::new();
        let texts = [
            (1, Value::Path(self.path.clone())),
            (2, Value::Str(self.interface.clone())),
            (3, Value::Str(self.member.clone())),
            (4, Value::Str(self.error.clone())),
            (6, Value::Str(self.destination.clone())),
            (8, Value::Signature(signature)),
        ];
        for (code, value) in texts {
            if value.as_str() != Some("") {
                fields.push(Value::Struct(vec![
                    Value::Byte(code),
                    Value::variant(value),
                ]));
            }
        }
        if self.reply_serial != 0 {
            let serial = Value::variant(Value::U32(self.reply_serial));
            fields.push(Value::Struct(vec![Value::Byte(5), serial]));
        }
        // The field array starts at offset 12; encode it from the 8-aligned
        // offset 8 so padding lands where the message needs it.
        let mut header = vec![0; 4];
        write(&mut header, &Value::Array("(yv)".into(), fields));
        out.extend(&header[4..]);
        out.resize(out.len().next_multiple_of(8), 0);
        out.extend(body);
        out
    }

    /// The total length of the message whose first 16 bytes `header` holds,
    /// or `None` if it is not a message within [`MESSAGE_LIMIT`].
    pub(crate) fn length(header: &[u8; 16]) -> Option<usize> {
        let big = match header[0] {
            b'l' => false,
            b'B' => true,
            _ => return None,
        };
        let word = |at: usize| {
            let bytes = header[at..at + 4].try_into().unwrap();
            match big {
                true => u32::from_be_bytes(bytes),
                false => u32::from_le_bytes(bytes),
            }
        };
        let fields = (word(12) as usize).next_multiple_of(8);
        let total = 16usize.checked_add(fields)?.checked_add(word(4) as usize)?;
        (total <= MESSAGE_LIMIT).then_some(total)
    }

    /// Decodes one complete message; `None` if it is malformed or uses a type
    /// this crate does not decode.
    pub(crate) fn decode(data: &[u8]) -> Option<Self> {
        let kind = match data[1] {
            1 => Kind::Call,
            2 => Kind::Return,
            3 => Kind::Error,
            4 => Kind::Signal,
            _ => return None,
        };
        let mut reader = Reader {
            data,
            pos: 4,
            big: data[0] == b'B',
        };
        let body_length = reader.u32()? as usize;
        let mut message = Self::new(kind);
        message.serial = reader.u32()?;
        message.no_reply = data[2] & 1 != 0;
        reader.pos = 12;
        let Value::Array(_, fields) = reader.value("a(yv)", 0)? else {
            return None;
        };
        let mut signature = String::new();
        for field in fields {
            let [Value::Byte(code), value] = field.items() else {
                return None;
            };
            let text = || value.as_str().map(str::to_owned);
            match code {
                1 => message.path = text()?,
                2 => message.interface = text()?,
                3 => message.member = text()?,
                4 => message.error = text()?,
                5 => message.reply_serial = value.as_u64()?.try_into().ok()?,
                6 => message.destination = text()?,
                7 => message.sender = text()?,
                8 => signature = text()?,
                // Unix descriptors are not supported.
                9 => return None,
                _ => {}
            }
        }
        reader.align(8)?;
        let end = reader.pos.checked_add(body_length)?;
        let mut rest = signature.as_str();
        while !rest.is_empty() {
            let (single, tail) = split(rest)?;
            message.body.push(reader.value(single, 0)?);
            rest = tail;
        }
        (reader.pos == end).then_some(message)
    }
}
