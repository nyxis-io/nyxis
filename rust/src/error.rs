use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum NxsError {
    BadMagic,
    UnknownSigil(char),
    BadEscape(char),
    OutOfBounds,
    DictMismatch,
    CircularLink,
    RecursionLimit,
    MacroUnresolved(String),
    ListTypeMismatch,
    Overflow,
    ParseError(String),
    IoError(String),
    /// Exit 4 — two records disagree on a key's sigil and policy is `error`.
    ConvertSchemaConflict(String),
    /// Exit 3 — malformed JSON/CSV/XML; byte offset is the position in the stream.
    ConvertParseError {
        offset: u64,
        msg: String,
    },
    /// Exit 3 — XML entity-expansion attack detected (billion-laughs etc.).
    ConvertEntityExpansion,
    /// Exit 3 — nesting depth exceeded `--max-depth` / `--xml-max-depth`.
    ConvertDepthExceeded,
}

impl fmt::Display for NxsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NxsError::BadMagic => write!(f, "ERR_BAD_MAGIC"),
            NxsError::UnknownSigil(c) => write!(f, "ERR_UNKNOWN_SIGIL: '{c}'"),
            NxsError::BadEscape(c) => write!(f, "ERR_BAD_ESCAPE: '\\{c}'"),
            NxsError::OutOfBounds => write!(f, "ERR_OUT_OF_BOUNDS"),
            NxsError::DictMismatch => write!(f, "ERR_DICT_MISMATCH"),
            NxsError::CircularLink => write!(f, "ERR_CIRCULAR_LINK"),
            NxsError::RecursionLimit => write!(f, "ERR_RECURSION_LIMIT"),
            NxsError::MacroUnresolved(s) => write!(f, "ERR_MACRO_UNRESOLVED: {s}"),
            NxsError::ListTypeMismatch => write!(f, "ERR_LIST_TYPE_MISMATCH"),
            NxsError::Overflow => write!(f, "ERR_OVERFLOW"),
            NxsError::ParseError(s) => write!(f, "ParseError: {s}"),
            NxsError::IoError(s) => write!(f, "IoError: {s}"),
            NxsError::ConvertSchemaConflict(s) => write!(f, "ERR_SCHEMA_CONFLICT: {s}"),
            NxsError::ConvertParseError { offset, msg } => {
                write!(f, "ERR_PARSE_ERROR at byte {offset}: {msg}")
            }
            NxsError::ConvertEntityExpansion => write!(f, "ERR_ENTITY_EXPANSION"),
            NxsError::ConvertDepthExceeded => write!(f, "ERR_DEPTH_EXCEEDED"),
        }
    }
}

impl std::error::Error for NxsError {}

pub type Result<T> = std::result::Result<T, NxsError>;
