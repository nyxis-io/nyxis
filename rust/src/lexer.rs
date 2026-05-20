use crate::error::{NxsError, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Sigils + their values
    Int(i64),
    Float(f64),
    Bool(bool),
    Keyword(String),
    Str(String),
    Time(i64), // unix nanoseconds
    Binary(Vec<u8>),
    Link(i32),
    Macro(String),
    Null,

    // Structure
    Ident(String),
    Colon,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    LParen,
    RParen,

    Eof,
}

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Lexer {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.input.get(self.pos).copied();
        self.pos += 1;
        c
    }

    fn skip_whitespace_and_comments(&mut self) {
        while let Some(c) = self.peek() {
            if c == '#' {
                while let Some(c) = self.peek() {
                    self.advance();
                    if c == '\n' {
                        break;
                    }
                }
            } else if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_while<F: Fn(char) -> bool>(&mut self, pred: F) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if pred(c) {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        s
    }

    fn read_string(&mut self) -> Result<String> {
        // opening `"` already consumed
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err(NxsError::ParseError("unterminated string".into())),
                Some('"') => break,
                Some('\\') => match self.advance() {
                    Some('\\') => s.push('\\'),
                    Some('"') => s.push('"'),
                    Some('n') => s.push('\n'),
                    Some('r') => s.push('\r'),
                    Some('t') => s.push('\t'),
                    Some('0') => s.push('\0'),
                    Some('u') => {
                        let hex: String = (0..4).filter_map(|_| self.advance()).collect();
                        let code = u32::from_str_radix(&hex, 16)
                            .map_err(|_| NxsError::ParseError(format!("bad \\u escape: {hex}")))?;
                        let ch = char::from_u32(code).ok_or_else(|| {
                            NxsError::ParseError(format!("invalid unicode: {code}"))
                        })?;
                        s.push(ch);
                    }
                    Some('U') => {
                        let hex: String = (0..8).filter_map(|_| self.advance()).collect();
                        let code = u32::from_str_radix(&hex, 16)
                            .map_err(|_| NxsError::ParseError(format!("bad \\U escape: {hex}")))?;
                        let ch = char::from_u32(code).ok_or_else(|| {
                            NxsError::ParseError(format!("invalid unicode: {code}"))
                        })?;
                        s.push(ch);
                    }
                    Some(c) => return Err(NxsError::BadEscape(c)),
                    None => return Err(NxsError::ParseError("unterminated escape".into())),
                },
                Some(c) => s.push(c),
            }
        }
        Ok(s)
    }

    fn read_binary(&mut self) -> Result<Vec<u8>> {
        // opening `<` already consumed; expect hex digits until `>`
        let mut hex = String::new();
        loop {
            match self.advance() {
                Some('>') => break,
                Some(c) if c.is_ascii_hexdigit() || c.is_whitespace() => {
                    if c.is_ascii_hexdigit() {
                        hex.push(c);
                    }
                }
                Some(c) => {
                    return Err(NxsError::ParseError(format!(
                        "unexpected char in binary: '{c}'"
                    )));
                }
                None => return Err(NxsError::ParseError("unterminated binary literal".into())),
            }
        }
        if hex.len() % 2 != 0 {
            return Err(NxsError::ParseError(
                "binary hex must have even number of digits".into(),
            ));
        }
        (0..hex.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&hex[i..i + 2], 16)
                    .map_err(|_| NxsError::ParseError(format!("bad hex byte: {}", &hex[i..i + 2])))
            })
            .collect()
    }

    fn read_macro_expr(&mut self) -> String {
        // consume to end of line or comma or closing brace
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c == '\n' || c == ',' || c == '}' {
                break;
            }
            s.push(c);
            self.advance();
        }
        s.trim().to_string()
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            match self.peek() {
                None => {
                    tokens.push(Token::Eof);
                    break;
                }
                Some(c) => {
                    self.advance();
                    let tok = match c {
                        '{' => Token::LBrace,
                        '}' => Token::RBrace,
                        '[' => Token::LBracket,
                        ']' => Token::RBracket,
                        '(' => Token::LParen,
                        ')' => Token::RParen,
                        ':' => Token::Colon,
                        ',' => Token::Comma,

                        // Sigils
                        '=' => {
                            let neg = if self.peek() == Some('-') {
                                self.advance();
                                true
                            } else {
                                false
                            };
                            let s = self.read_while(|c| c.is_ascii_digit());
                            let n: i64 = s
                                .parse()
                                .map_err(|_| NxsError::ParseError(format!("bad int: {s}")))?;
                            Token::Int(if neg { -n } else { n })
                        }
                        '~' => {
                            let neg = if self.peek() == Some('-') {
                                self.advance();
                                true
                            } else {
                                false
                            };
                            let s = self.read_while(|c| {
                                c.is_ascii_digit()
                                    || c == '.'
                                    || c == 'e'
                                    || c == 'E'
                                    || c == '+'
                                    || c == '-'
                            });
                            let f: f64 = s
                                .parse()
                                .map_err(|_| NxsError::ParseError(format!("bad float: {s}")))?;
                            Token::Float(if neg { -f } else { f })
                        }
                        '?' => {
                            let s = self.read_while(|c| c.is_alphabetic());
                            match s.as_str() {
                                "true" => Token::Bool(true),
                                "false" => Token::Bool(false),
                                _ => return Err(NxsError::ParseError(format!("bad bool: {s}"))),
                            }
                        }
                        '$' => {
                            let s = self.read_while(|c| c.is_alphanumeric() || c == '_');
                            Token::Keyword(s)
                        }
                        '"' => Token::Str(self.read_string()?),
                        '@' => {
                            // peek: if digit, it's a timestamp; otherwise it's a macro ref (handled in parser)
                            if self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                                // parse ISO-8601 date as nanoseconds: YYYY-MM-DD or full RFC3339
                                let s = self.read_while(|c| {
                                    !c.is_whitespace() && c != ',' && c != '}' && c != ']'
                                });
                                let ns = parse_temporal(&s)?;
                                Token::Time(ns)
                            } else {
                                // macro ref — return @ + ident as a raw string for the macro parser
                                let ident = self.read_while(|c| c.is_alphanumeric() || c == '_');
                                Token::Macro(format!("@{ident}"))
                            }
                        }
                        '<' => Token::Binary(self.read_binary()?),
                        '&' => {
                            let neg = if self.peek() == Some('-') {
                                self.advance();
                                true
                            } else {
                                false
                            };
                            let s = self.read_while(|c| c.is_ascii_digit());
                            let n: i32 = s.parse().map_err(|_| {
                                NxsError::ParseError(format!("bad link offset: {s}"))
                            })?;
                            Token::Link(if neg { -n } else { n })
                        }
                        '!' => Token::Macro(self.read_macro_expr()),
                        '^' => Token::Null,

                        // Identifier (key name)
                        c if c.is_alphabetic() || c == '_' => {
                            let mut s = c.to_string();
                            s.push_str(
                                &self.read_while(|c| c.is_alphanumeric() || c == '_' || c == '-'),
                            );
                            Token::Ident(s)
                        }

                        other => return Err(NxsError::UnknownSigil(other)),
                    };
                    tokens.push(tok);
                }
            }
        }
        Ok(tokens)
    }
}

fn parse_temporal(s: &str) -> Result<i64> {
    // Support YYYY-MM-DD
    if s.len() == 10 && s.chars().nth(4) == Some('-') {
        let year: i64 = s[0..4]
            .parse()
            .map_err(|_| NxsError::ParseError(format!("bad date: {s}")))?;
        let month: i64 = s[5..7]
            .parse()
            .map_err(|_| NxsError::ParseError(format!("bad date: {s}")))?;
        let day: i64 = s[8..10]
            .parse()
            .map_err(|_| NxsError::ParseError(format!("bad date: {s}")))?;
        // Days since epoch (very simplified, good enough for POC)
        let days = days_since_epoch(year, month, day);
        return days
            .checked_mul(86_400_000_000_000i64)
            .ok_or_else(|| NxsError::ParseError(format!("temporal overflow: {s}")))
            .map(Some)
            .map(|v| v.unwrap());
    }
    // Support raw nanosecond integer
    s.parse::<i64>()
        .map_err(|_| NxsError::ParseError(format!("bad temporal: {s}")))
}

fn days_since_epoch(year: i64, month: i64, day: i64) -> i64 {
    // Julian Day Number → days since Unix epoch (1970-01-01)
    let a = (14 - month) / 12;
    let y = year + 4800 - a;
    let m = month + 12 * a - 3;
    let jdn = day + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045;
    jdn - 2_440_588 // JDN of 1970-01-01
}
