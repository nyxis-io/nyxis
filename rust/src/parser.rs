use crate::error::{NxsError, Result};
use crate::lexer::Token;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Keyword(String),
    Str(String),
    Time(i64),
    Binary(Vec<u8>),
    Link(i32),
    Macro(String),
    Null,
    Object(Vec<Field>),
    List(Vec<Value>),
}

#[derive(Debug, Clone)]
pub struct Field {
    pub key: String,
    pub value: Value,
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
}

const MAX_DEPTH: usize = 64;

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            pos: 0,
            depth: 0,
        }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> &Token {
        let t = self.tokens.get(self.pos).unwrap_or(&Token::Eof);
        self.pos += 1;
        t
    }

    fn expect(&mut self, expected: &Token) -> Result<()> {
        let t = self.advance().clone();
        if &t == expected {
            Ok(())
        } else {
            Err(NxsError::ParseError(format!(
                "expected {expected:?}, got {t:?}"
            )))
        }
    }

    pub fn parse_file(&mut self) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        while self.peek() != &Token::Eof {
            fields.push(self.parse_field()?);
        }
        Ok(fields)
    }

    fn parse_field(&mut self) -> Result<Field> {
        let key = match self.advance().clone() {
            Token::Ident(s) => s,
            other => {
                return Err(NxsError::ParseError(format!(
                    "expected field key, got {other:?}"
                )));
            }
        };

        // Colon is optional if followed by `{` (shorthand object syntax)
        if self.peek() == &Token::Colon {
            self.advance();
        }

        let value = self.parse_value()?;
        Ok(Field { key, value })
    }

    fn parse_value(&mut self) -> Result<Value> {
        match self.peek().clone() {
            Token::LBrace => self.parse_object(),
            Token::LBracket => self.parse_list(),
            Token::Int(n) => {
                self.advance();
                Ok(Value::Int(n))
            }
            Token::Float(f) => {
                self.advance();
                Ok(Value::Float(f))
            }
            Token::Bool(b) => {
                self.advance();
                Ok(Value::Bool(b))
            }
            Token::Keyword(s) => {
                self.advance();
                Ok(Value::Keyword(s))
            }
            Token::Str(s) => {
                self.advance();
                Ok(Value::Str(s))
            }
            Token::Time(ns) => {
                self.advance();
                Ok(Value::Time(ns))
            }
            Token::Binary(b) => {
                self.advance();
                Ok(Value::Binary(b))
            }
            Token::Link(n) => {
                self.advance();
                Ok(Value::Link(n))
            }
            Token::Macro(s) => {
                self.advance();
                Ok(Value::Macro(s))
            }
            Token::Null => {
                self.advance();
                Ok(Value::Null)
            }
            other => Err(NxsError::ParseError(format!(
                "unexpected token for value: {other:?}"
            ))),
        }
    }

    fn parse_object(&mut self) -> Result<Value> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(NxsError::RecursionLimit);
        }
        self.expect(&Token::LBrace)?;
        let mut fields = Vec::new();
        while self.peek() != &Token::RBrace && self.peek() != &Token::Eof {
            fields.push(self.parse_field()?);
            // optional comma between fields
            if self.peek() == &Token::Comma {
                self.advance();
            }
        }
        self.expect(&Token::RBrace)?;
        self.depth -= 1;
        Ok(Value::Object(fields))
    }

    fn parse_list(&mut self) -> Result<Value> {
        self.expect(&Token::LBracket)?;
        let mut elems = Vec::new();
        let mut sigil_tag: Option<&'static str> = None;

        while self.peek() != &Token::RBracket && self.peek() != &Token::Eof {
            let v = self.parse_value()?;
            let tag = sigil_name(&v);
            match sigil_tag {
                None => sigil_tag = Some(tag),
                Some(expected) if expected != tag => return Err(NxsError::ListTypeMismatch),
                _ => {}
            }
            elems.push(v);
            if self.peek() == &Token::Comma {
                self.advance();
            }
        }
        self.expect(&Token::RBracket)?;
        Ok(Value::List(elems))
    }
}

fn sigil_name(v: &Value) -> &'static str {
    match v {
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Bool(_) => "bool",
        Value::Keyword(_) => "keyword",
        Value::Str(_) => "str",
        Value::Time(_) => "time",
        Value::Binary(_) => "binary",
        Value::Link(_) => "link",
        Value::Macro(_) => "macro",
        Value::Null => "null",
        Value::Object(_) => "object",
        Value::List(_) => "list",
    }
}
