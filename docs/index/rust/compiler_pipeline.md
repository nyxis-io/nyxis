---
room: compiler_pipeline
subdomain: rust
source_paths: rust/src/lexer.rs, rust/src/parser.rs, rust/src/compiler.rs, rust/src/error.rs
see_also: rust/writer_decoder.md, rust/convert.md
hot_paths: compiler.rs, lexer.rs
architectural_health: normal
security_tier: normal
---

# rust/ — Compiler Pipeline

Subdomain: rust/
Source paths: rust/src/lexer.rs, rust/src/parser.rs, rust/src/compiler.rs, rust/src/error.rs

## TASK → LOAD

| Task | Load |
|------|------|
| Compile a .nxs source file to .nxb bytes | compiler_pipeline.md |
| Tokenize .nxs text / debug lexer output | compiler_pipeline.md |
| Parse .nxs AST / debug parse errors | compiler_pipeline.md |
| Understand or add an NxsError variant | compiler_pipeline.md |
| Add a new sigil type to the language | compiler_pipeline.md |

---

# compiler.rs

DOES: Two-pass compiler that transforms a parsed AST (`Vec<Field>`) into the NXB binary format. First pass interns all key strings into a global dictionary; second pass emits the preamble, schema header, LEB128-bitmask object headers, value payloads, and tail-index.
SYMBOLS:
- Compiler::new() -> Compiler
- Compiler::collect_keys(&mut self, fields: &[Field])
- Compiler::compile(&mut self, fields: &[Field]) -> Result<Vec<u8>>
- encode_value(v: &Value) -> Result<Vec<u8>>
- encode_list(elems: &[Value]) -> Result<Vec<u8>>
- build_bitmask(present_indices: &[usize], total_keys: usize) -> Vec<u8>
- resolve_macro(value: &Value, scope: &[Field]) -> Result<Value>
- murmur3_64(data: &[u8]) -> u64
TYPE: Compiler { dict: Vec<String>, key_map: HashMap<String, usize> }
DEPENDS: crate::error, crate::parser
PATTERNS: two-pass-compile, leb128-bitmask, rule-of-8-alignment
USE WHEN: Converting an in-memory AST to `.nxb` bytes; prefer `crate::writer` (NxsWriter) for hot-path fixture generation that bypasses text parsing entirely.
DISAMBIGUATION: `murmur3_64` also appears as a standalone function in `nxs.c` (`c/reader.md`) and `nxs.go` (`go/reader.md`). Those are independent re-implementations for DictHash verification in their respective languages. If the question is about cross-language hash parity or the spec hash algorithm itself, load `c/reader.md` or `go/reader.md` alongside this room.

---

# error.rs

DOES: Defines the unified `NxsError` enum covering all parse, binary-format, macro, and format-conversion error conditions, plus the `Result<T>` type alias used throughout the crate.
SYMBOLS:
- Types: NxsError, Result<T>
TYPE: NxsError { BadMagic, UnknownSigil(char), BadEscape(char), OutOfBounds, DictMismatch, CircularLink, RecursionLimit, MacroUnresolved(String), ListTypeMismatch, Overflow, ParseError(String), IoError(String), ConvertSchemaConflict(String), ConvertParseError { offset, msg }, ConvertEntityExpansion, ConvertDepthExceeded }
DEPENDS: (none — leaf module)
PATTERNS: error-enum, type-alias-result
USE WHEN: Matching or constructing errors anywhere in the crate; error codes like `ERR_BAD_MAGIC` and `ERR_DICT_MISMATCH` displayed here are the spec-mandated strings checked by conformance runners.

---

# lexer.rs

DOES: Tokenizes `.nxs` source text into a flat `Vec<Token>`, recognising all nine sigil prefixes (`= ~ ? $ " @ < & ^`), structural punctuation, and identifiers; skips `#`-line comments.
SYMBOLS:
- Lexer::new(input: &str) -> Lexer
- Lexer::tokenize(&mut self) -> Result<Vec<Token>>
TYPE: Token { Int(i64), Float(f64), Bool(bool), Keyword(String), Str(String), Time(i64), Binary(Vec<u8>), Link(i32), Macro(String), Null, Ident(String), Colon, LBrace, RBrace, LBracket, RBracket, Comma, LParen, RParen, Eof }
DEPENDS: crate::error
PATTERNS: sigil-dispatch, hand-written-lexer
USE WHEN: First stage of the compile pipeline; feed its output directly to `parser::Parser::new`.

---

# parser.rs

DOES: Consumes a `Vec<Token>` from the lexer and produces a `Vec<Field>` AST, enforcing list-type homogeneity and a maximum nesting depth of 64.
SYMBOLS:
- Parser::new(tokens: Vec<Token>) -> Parser
- Parser::parse_file(&mut self) -> Result<Vec<Field>>
TYPE: Value { Int(i64), Float(f64), Bool(bool), Keyword(String), Str(String), Time(i64), Binary(Vec<u8>), Link(i32), Macro(String), Null, Object(Vec<Field>), List(Vec<Value>) }
DEPENDS: crate::error, crate::lexer
PATTERNS: recursive-descent, depth-guard
USE WHEN: Second stage of the compile pipeline; its `Vec<Field>` output is the input type for `compiler::Compiler::compile` and `compiler::Compiler::collect_keys`.
