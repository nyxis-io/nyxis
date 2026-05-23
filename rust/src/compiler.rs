use crate::consts::{
    FLAG_SCHEMA_EMBEDDED, MAGIC_FILE, MAGIC_FOOTER, MAGIC_LIST, MAGIC_OBJ, SIGIL_BINARY,
    SIGIL_BOOL, SIGIL_FLOAT, SIGIL_INT, SIGIL_KEYWORD, SIGIL_LINK, SIGIL_NULL, SIGIL_STR,
    SIGIL_TIME, VERSION,
};
use crate::error::{NxsError, Result};
use crate::parser::{Field, Value};
use std::collections::HashMap;

pub struct Compiler {
    dict: Vec<String>,               // key index → key name
    key_map: HashMap<String, usize>, // key name → index
    /// Per-slot TypeManifest sigil (0 = unset → defaults to SIGIL_STR in schema).
    slot_sigils: Vec<u8>,
}

impl Compiler {
    pub fn new() -> Self {
        Compiler {
            dict: Vec::new(),
            key_map: HashMap::new(),
            slot_sigils: Vec::new(),
        }
    }

    // First pass: collect all unique keys into the global dictionary
    pub fn collect_keys(&mut self, fields: &[Field]) {
        for field in fields {
            let idx = self.intern_key(&field.key);
            self.mark_slot_sigil(idx, value_sigil_byte(&field.value));
            self.collect_keys_from_value(&field.value);
        }
    }

    fn collect_keys_from_value(&mut self, value: &Value) {
        match value {
            Value::Object(fields) => {
                for field in fields {
                    let idx = self.intern_key(&field.key);
                    self.mark_slot_sigil(idx, value_sigil_byte(&field.value));
                    self.collect_keys_from_value(&field.value);
                }
            }
            Value::List(elems) => {
                for e in elems {
                    self.collect_keys_from_value(e);
                }
            }
            _ => {}
        }
    }

    fn intern_key(&mut self, key: &str) -> usize {
        if let Some(&idx) = self.key_map.get(key) {
            return idx;
        }
        let idx = self.dict.len();
        self.dict.push(key.to_string());
        self.slot_sigils.push(0);
        self.key_map.insert(key.to_string(), idx);
        idx
    }

    fn mark_slot_sigil(&mut self, slot: usize, sigil: u8) {
        if slot >= self.slot_sigils.len() {
            return;
        }
        let cur = self.slot_sigils[slot];
        if cur == 0 || (cur == SIGIL_NULL && sigil != SIGIL_NULL) {
            self.slot_sigils[slot] = sigil;
        }
    }

    pub fn compile(&mut self, fields: &[Field]) -> Result<Vec<u8>> {
        self.collect_keys(fields);

        let mut data_sector: Vec<u8> = Vec::new();
        // Top-level fields are wrapped into a single root object
        let root_bytes = self.encode_object(fields)?;
        data_sector.extend_from_slice(&root_bytes);

        let schema_bytes = self.encode_schema();
        let tail_ptr: u64 = 32 + schema_bytes.len() as u64 + data_sector.len() as u64;
        let tail_index = self.encode_tail_index(32 + schema_bytes.len() as u64, tail_ptr);
        let dict_hash = murmur3_64(&schema_bytes);

        let preamble = self.encode_preamble(dict_hash, FLAG_SCHEMA_EMBEDDED);

        let mut out = Vec::new();
        out.extend_from_slice(&preamble);
        out.extend_from_slice(&schema_bytes);
        out.extend_from_slice(&data_sector);
        out.extend_from_slice(&tail_index);
        Ok(out)
    }

    fn encode_preamble(&self, dict_hash: u64, flags: u16) -> Vec<u8> {
        let mut b = Vec::with_capacity(32);
        b.extend_from_slice(&MAGIC_FILE.to_le_bytes()); // 0..4
        b.extend_from_slice(&VERSION.to_le_bytes()); // 4..6
        b.extend_from_slice(&flags.to_le_bytes()); // 6..8
        b.extend_from_slice(&dict_hash.to_le_bytes()); // 8..16
                                                       // v1.1 streamable format always writes tail_ptr=0 here; the actual
                                                       // tail pointer is stored in the footer FooterTailPtr field instead.
        b.extend_from_slice(&0u64.to_le_bytes()); // 16..24 tail_ptr (always 0)
        b.extend_from_slice(&0u64.to_le_bytes()); // 24..32 reserved
        b
    }

    fn encode_schema(&self) -> Vec<u8> {
        let mut b = Vec::new();
        let key_count = self.dict.len() as u16;
        b.extend_from_slice(&key_count.to_le_bytes());

        for (i, _) in self.dict.iter().enumerate() {
            let s = self.slot_sigils.get(i).copied().unwrap_or(0);
            b.push(if s == 0 { SIGIL_STR } else { s });
        }

        // StringPool: null-terminated names
        for key in &self.dict {
            b.extend_from_slice(key.as_bytes());
            b.push(0x00);
        }

        // Pad to 8-byte boundary
        while b.len() % 8 != 0 {
            b.push(0x00);
        }
        b
    }

    fn encode_object(&self, fields: &[Field]) -> Result<Vec<u8>> {
        // Resolve macro fields first
        let resolved: Vec<(usize, Value)> = fields
            .iter()
            .map(|f| {
                let idx = *self
                    .key_map
                    .get(&f.key)
                    .ok_or_else(|| NxsError::ParseError(format!("key not in dict: {}", f.key)))?;
                let v = resolve_macro(&f.value, fields)?;
                Ok((idx, v))
            })
            .collect::<Result<Vec<_>>>()?;

        // Build bitmask
        let mask = build_bitmask(
            &resolved.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            self.dict.len(),
        );

        // Encode each value
        let mut value_bufs: Vec<Vec<u8>> = Vec::new();
        for (_, v) in &resolved {
            value_bufs.push(encode_value(v)?);
        }

        // Build offset table — offsets relative to object start (Magic byte)
        // Object structure: [Magic 4][Length 4][Bitmask N][OffsetTable M*2][values...]
        let header_size = 4 + 4; // magic + length
        let bitmask_size = mask.len();
        let offset_table_size = resolved.len() * 2; // normal mode: u16 each
        let data_start = header_size + bitmask_size + offset_table_size;

        // Align data_start to 8
        let data_start_aligned = align8(data_start);
        let align_padding = data_start_aligned - data_start;

        let mut offsets: Vec<u16> = Vec::new();
        let mut cursor = data_start_aligned;
        for buf in &value_bufs {
            offsets.push(cursor as u16);
            cursor += buf.len();
        }

        let total_len = cursor;

        let mut obj = Vec::with_capacity(total_len);
        obj.extend_from_slice(&MAGIC_OBJ.to_le_bytes());
        obj.extend_from_slice(&(total_len as u32).to_le_bytes());
        obj.extend_from_slice(&mask);
        for off in &offsets {
            obj.extend_from_slice(&off.to_le_bytes());
        }
        for _ in 0..align_padding {
            obj.push(0x00);
        }
        for buf in &value_bufs {
            obj.extend_from_slice(buf);
        }
        Ok(obj)
    }

    fn encode_tail_index(&self, data_sector_start: u64, tail_ptr: u64) -> Vec<u8> {
        // For the root object there is exactly one top-level record
        let mut b = Vec::new();
        let entry_count: u32 = 1;
        b.extend_from_slice(&entry_count.to_le_bytes());
        // KeyID 0 (root), absolute offset = data_sector_start
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&data_sector_start.to_le_bytes());
        b.extend_from_slice(&tail_ptr.to_le_bytes());
        b.extend_from_slice(&MAGIC_FOOTER.to_le_bytes());
        b
    }
}

// --- Encoding helpers ---

fn encode_value(v: &Value) -> Result<Vec<u8>> {
    match v {
        Value::Int(n) => {
            let mut b = Vec::with_capacity(8);
            b.extend_from_slice(&n.to_le_bytes());
            Ok(b)
        }
        Value::Float(f) => {
            let mut b = Vec::with_capacity(8);
            b.extend_from_slice(&f.to_le_bytes());
            Ok(b)
        }
        Value::Bool(bl) => {
            let mut b = vec![if *bl { 0x01u8 } else { 0x00u8 }];
            // 7 bytes padding to maintain 8-byte alignment for next field
            b.extend_from_slice(&[0u8; 7]);
            Ok(b)
        }
        Value::Keyword(_) => Err(NxsError::UnsupportedFieldType),
        Value::Str(s) => {
            let bytes = s.as_bytes();
            let len = bytes.len() as u32;
            let mut b = Vec::new();
            b.extend_from_slice(&len.to_le_bytes());
            b.extend_from_slice(bytes);
            pad_to_8(&mut b);
            Ok(b)
        }
        Value::Time(ns) => {
            let mut b = Vec::with_capacity(8);
            b.extend_from_slice(&ns.to_le_bytes());
            Ok(b)
        }
        Value::Binary(raw) => {
            let len = raw.len() as u32;
            let mut b = Vec::new();
            b.extend_from_slice(&len.to_le_bytes());
            b.extend_from_slice(raw);
            pad_to_8(&mut b);
            Ok(b)
        }
        Value::Link(off) => {
            let mut b = Vec::with_capacity(8);
            b.extend_from_slice(&off.to_le_bytes());
            b.extend_from_slice(&[0u8; 4]); // pad to 8
            Ok(b)
        }
        Value::Null => {
            // Null is zero-width: the bitmask bit and offset-table slot are sufficient
            // to distinguish explicit Null from an absent field.  No payload bytes are
            // emitted.  (An earlier draft of the spec incorrectly said "offset points
            // to a single 0x00 byte" — see SPEC.md §5.4 conformance note.)
            Ok(vec![])
        }
        Value::Object(fields) => {
            // Nested object: recursively compile with a fresh compiler that shares the parent dict
            // For POC we use a standalone compiler — a real impl would share the global dict
            let mut inner = Compiler::new();
            inner.collect_keys(fields);
            // Copy parent dict entries
            inner.dict = fields.iter().map(|f| f.key.clone()).collect();
            inner.key_map = inner
                .dict
                .iter()
                .cloned()
                .enumerate()
                .map(|(i, k)| (k, i))
                .collect();
            inner.encode_object(fields)
        }
        Value::List(elems) => encode_list(elems),
        Value::Macro(_) => Err(NxsError::MacroUnresolved(
            "unresolved macro in encode".into(),
        )),
    }
}

fn encode_list(elems: &[Value]) -> Result<Vec<u8>> {
    if elems.is_empty() {
        let mut b = Vec::new();
        b.extend_from_slice(&MAGIC_LIST.to_le_bytes()); // 4
        b.extend_from_slice(&16u32.to_le_bytes()); // length=16
        b.push(0x00); // sigil (none)
        b.extend_from_slice(&0u32.to_le_bytes()); // ElemCount
        b.extend_from_slice(&[0u8; 3]); // padding
        return Ok(b);
    }

    let sigil_byte = value_sigil_byte(elems.first().unwrap());

    let mut elem_bufs: Vec<Vec<u8>> = elems
        .iter()
        .map(|e| {
            if value_sigil_byte(e) != sigil_byte {
                return Err(NxsError::ListTypeMismatch);
            }
            encode_value(e)
        })
        .collect::<Result<Vec<_>>>()?;

    // List header is 16 bytes: Magic(4) + Length(4) + ElemSigil(1) + ElemCount(4) + Padding(3)
    let data_len: usize = elem_bufs.iter().map(|b| b.len()).sum();
    let total_len = 16 + data_len;

    let mut b = Vec::with_capacity(total_len);
    b.extend_from_slice(&MAGIC_LIST.to_le_bytes());
    b.extend_from_slice(&(total_len as u32).to_le_bytes());
    b.push(sigil_byte);
    b.extend_from_slice(&(elems.len() as u32).to_le_bytes());
    b.extend_from_slice(&[0u8; 3]); // padding to align data to offset 16
    for buf in &mut elem_bufs {
        b.append(buf);
    }
    Ok(b)
}

fn value_sigil_byte(v: &Value) -> u8 {
    match v {
        Value::Int(_) => SIGIL_INT,
        Value::Float(_) => SIGIL_FLOAT,
        Value::Bool(_) => SIGIL_BOOL,
        Value::Keyword(_) => SIGIL_KEYWORD,
        Value::Str(_) => SIGIL_STR,
        Value::Time(_) => SIGIL_TIME,
        Value::Binary(_) => SIGIL_BINARY,
        Value::Link(_) => SIGIL_LINK,
        Value::Null => SIGIL_NULL,
        Value::Object(_) => b'O',
        Value::List(_) => b'L',
        Value::Macro(_) => b'!',
    }
}

fn pad_to_8(b: &mut Vec<u8>) {
    while b.len() % 8 != 0 {
        b.push(0x00);
    }
}

fn align8(n: usize) -> usize {
    (n + 7) & !7
}

// Build LEB128 continuation-bit bitmask encoding the presence of given key indices
fn build_bitmask(present_indices: &[usize], total_keys: usize) -> Vec<u8> {
    if total_keys == 0 {
        return vec![0x00];
    }
    let mut bits = vec![false; total_keys];
    for &idx in present_indices {
        if idx < total_keys {
            bits[idx] = true;
        }
    }
    // Encode in groups of 7 bits with LEB128 continuation
    let mut result = Vec::new();
    let mut i = 0;
    while i < bits.len() {
        let chunk: Vec<bool> = bits[i..bits.len().min(i + 7)].to_vec();
        let mut byte: u8 = 0;
        for (bit_pos, &set) in chunk.iter().enumerate() {
            if set {
                byte |= 1 << bit_pos;
            }
        }
        let has_more = i + 7 < bits.len();
        if has_more {
            byte |= 0x80;
        }
        result.push(byte);
        i += 7;
    }
    result
}

// Minimal macro resolution: handle @key references and string concatenation
fn resolve_macro(value: &Value, scope: &[Field]) -> Result<Value> {
    match value {
        Value::Macro(expr) => eval_macro(expr, scope),
        other => Ok(other.clone()),
    }
}

fn eval_macro(expr: &str, scope: &[Field]) -> Result<Value> {
    let expr = expr.trim();

    // @key reference
    if let Some(key) = expr.strip_prefix('@') {
        return scope
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.value.clone())
            .ok_or_else(|| NxsError::MacroUnresolved(format!("@{key} not found in scope")));
    }

    // now() built-in
    if expr == "now()" {
        // Return 0 for deterministic output in POC; real impl would use SystemTime
        return Ok(Value::Time(0));
    }

    // String/int literal passthrough
    if expr.starts_with('"') && expr.ends_with('"') {
        let inner = &expr[1..expr.len() - 1];
        return Ok(Value::Str(inner.to_string()));
    }
    if let Ok(n) = expr.parse::<i64>() {
        return Ok(Value::Int(n));
    }
    if let Ok(f) = expr.parse::<f64>() {
        return Ok(Value::Float(f));
    }

    // String concatenation: split on ` + `
    if expr.contains(" + ") {
        let parts: Vec<&str> = expr.splitn(2, " + ").collect();
        let left = eval_macro(parts[0].trim(), scope)?;
        let right = eval_macro(parts[1].trim(), scope)?;
        return match (left, right) {
            (Value::Str(a), Value::Str(b)) => Ok(Value::Str(a + &b)),
            (Value::Int(a), Value::Int(b)) => {
                a.checked_add(b).map(Value::Int).ok_or(NxsError::Overflow)
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            _ => Err(NxsError::MacroUnresolved(format!(
                "incompatible types in +: {expr}"
            ))),
        };
    }

    Err(NxsError::MacroUnresolved(format!(
        "cannot evaluate: {expr}"
    )))
}

// MurmurHash3 64-bit (simplified finalizer-based version for POC)
fn murmur3_64(data: &[u8]) -> u64 {
    let mut h: u64 = 0x9368_1D62_5531_3A99;
    for chunk in data.chunks(8) {
        let mut k = 0u64;
        for (i, &b) in chunk.iter().enumerate() {
            k |= (b as u64) << (i * 8);
        }
        k = k.wrapping_mul(0xFF51AFD7ED558CCD);
        k ^= k >> 33;
        h ^= k;
        h = h.wrapping_mul(0xC4CEB9FE1A85EC53);
        h ^= h >> 33;
    }
    h ^= data.len() as u64;
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51AFD7ED558CCD);
    h ^= h >> 33;
    h
}
