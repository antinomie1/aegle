use crate::{Error, Span};

#[derive(Debug)]
pub(crate) enum Kind<'a> {
    Identifier(&'a str),
    String(String),
    Number(f32),
    Length(f32),
    Color([u8; 4]),
    Open,
    Close,
    Colon,
    Separator,
    End,
}

#[derive(Debug)]
pub(crate) struct Token<'a> {
    pub kind: Kind<'a>,
    pub span: Span,
}

pub(crate) struct Lexer<'a> {
    source: &'a str,
    cursor: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source, cursor: 0 }
    }

    pub fn next(&mut self) -> Result<Token<'a>, Error> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r') => self.cursor += 1,
                Some(b'/') if self.source[self.cursor..].starts_with("//") => {
                    self.cursor += self.source[self.cursor..]
                        .find('\n')
                        .unwrap_or(self.source.len() - self.cursor);
                }
                _ => break,
            }
        }
        let start = self.cursor;
        let kind = match self.peek() {
            None => Kind::End,
            Some(b'{') => self.single(Kind::Open),
            Some(b'}') => self.single(Kind::Close),
            Some(b':') => self.single(Kind::Colon),
            Some(b'\n' | b';') => self.single(Kind::Separator),
            Some(b'"') => Kind::String(self.string()?),
            Some(b'#') => self.color()?,
            Some(b'-' | b'0'..=b'9') => self.number()?,
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_') => {
                self.cursor += 1;
                while self
                    .peek()
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                {
                    self.cursor += 1;
                }
                Kind::Identifier(&self.source[start..self.cursor])
            }
            _ => {
                return Err(self.error(
                    start,
                    "unsupported syntax; expected a literal or declaration (expressions are not supported)",
                ));
            }
        };
        Ok(Token {
            kind,
            span: Span {
                start,
                end: self.cursor,
            },
        })
    }

    fn single(&mut self, kind: Kind<'a>) -> Kind<'a> {
        self.cursor += 1;
        kind
    }

    fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.cursor).copied()
    }

    fn error(&self, start: usize, message: &str) -> Error {
        let end = if self.cursor > start {
            self.cursor
        } else {
            start
                + self.source[start..]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8)
        };
        Error::new(Span { start, end }, message)
    }

    fn digits(&mut self) -> bool {
        let start = self.cursor;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.cursor += 1;
        }
        self.cursor != start
    }

    fn number(&mut self) -> Result<Kind<'a>, Error> {
        let start = self.cursor;
        if self.peek() == Some(b'-') {
            self.cursor += 1;
        }
        if !self.digits() {
            return Err(self.error(start, "expected digits in a finite number"));
        }
        if self.peek() == Some(b'.') {
            self.cursor += 1;
            if !self.digits() {
                return Err(self.error(start, "expected digits after the decimal point"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.cursor += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.cursor += 1;
            }
            if !self.digits() {
                return Err(self.error(start, "expected digits in the exponent"));
            }
        }
        let number = self.source[start..self.cursor]
            .parse::<f32>()
            .map_err(|_| self.error(start, "invalid number"))?;
        if !number.is_finite() {
            return Err(self.error(start, "number must be finite and fit in f32"));
        }
        if self.source[self.cursor..].starts_with("dp") {
            self.cursor += 2;
            Ok(Kind::Length(number))
        } else {
            Ok(Kind::Number(number))
        }
    }

    fn color(&mut self) -> Result<Kind<'a>, Error> {
        let start = self.cursor;
        self.cursor += 1;
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.cursor += 1;
        }
        let digits = &self.source[start + 1..self.cursor];
        let error = || {
            self.error(
                start,
                "color requires exactly six or eight hexadecimal digits",
            )
        };
        if !matches!(digits.len(), 6 | 8) {
            return Err(error());
        }
        let mut channels = [0, 0, 0, 255];
        for (channel, bytes) in channels.iter_mut().zip(digits.as_bytes().chunks_exact(2)) {
            let high = (bytes[0] as char).to_digit(16).ok_or_else(error)?;
            let low = (bytes[1] as char).to_digit(16).ok_or_else(error)?;
            *channel = (high * 16 + low) as u8;
        }
        Ok(Kind::Color(channels))
    }

    fn string(&mut self) -> Result<String, Error> {
        let start = self.cursor;
        self.cursor += 1;
        let mut value = String::new();
        loop {
            match self.peek() {
                Some(b'"') => {
                    self.cursor += 1;
                    return Ok(value);
                }
                Some(b'\\') => {
                    self.cursor += 1;
                    let escaped = match self.peek() {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'/') => '/',
                        Some(b'b') => '\u{0008}',
                        Some(b'f') => '\u{000c}',
                        Some(b'n') => '\n',
                        Some(b'r') => '\r',
                        Some(b't') => '\t',
                        Some(b'u') => {
                            self.cursor += 1;
                            value.push(self.unicode_escape()?);
                            continue;
                        }
                        _ => return Err(self.error(self.cursor, "invalid JSON string escape")),
                    };
                    self.cursor += 1;
                    value.push(escaped);
                }
                Some(0..=0x1f) => {
                    return Err(self.error(self.cursor, "control characters must be escaped"));
                }
                Some(_) => {
                    let ch = self.source[self.cursor..].chars().next().unwrap();
                    value.push(ch);
                    self.cursor += ch.len_utf8();
                }
                None => return Err(self.error(start, "unterminated string")),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let start = self.cursor;
        let mut value = 0;
        for _ in 0..4 {
            let digit = self
                .peek()
                .and_then(|byte| (byte as char).to_digit(16))
                .ok_or_else(|| self.error(start, "expected four hexadecimal escape digits"))?;
            self.cursor += 1;
            value = value * 16 + digit;
        }
        Ok(value)
    }

    fn unicode_escape(&mut self) -> Result<char, Error> {
        let start = self.cursor;
        let high = self.hex4()?;
        let value = if (0xd800..=0xdbff).contains(&high) {
            if !self.source[self.cursor..].starts_with("\\u") {
                return Err(self.error(start, "high surrogate requires a low surrogate escape"));
            }
            self.cursor += 2;
            let low = self.hex4()?;
            if !(0xdc00..=0xdfff).contains(&low) {
                return Err(self.error(start, "invalid low surrogate"));
            }
            0x10000 + ((high - 0xd800) << 10) + low - 0xdc00
        } else {
            high
        };
        char::from_u32(value).ok_or_else(|| self.error(start, "unpaired low surrogate"))
    }
}
