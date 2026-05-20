//! Two-pass streaming sigil inference.
//!
//! Pass 1: iterate source records, keep a per-key lattice state. Pass 2 is
//! driven by the caller (json_in/csv_in/xml_in) using the frozen schema.
//!
//! Priority lattice (from spec inference_rules.priority):
//!   = (int)    > ~ (float) > ? (bool) > @ (time) > < (hex) > ^ (null) > " (string)
//!
//! Tie-breaks (spec § tie_breaks):
//!   - 0/1 columns → int (not bool); int rule fires first
//!   - hex requires length ≥ 16, even length, all hex chars
//!   - fallback is always string

use super::{ConflictPolicy, InferredKey, InferredSchema};
use crate::error::{NxsError, Result};

// NXS sigil bytes
pub const SIGIL_INT: u8 = b'=';
pub const SIGIL_FLOAT: u8 = b'~';
pub const SIGIL_BOOL: u8 = b'?';
pub const SIGIL_TIME: u8 = b'@';
pub const SIGIL_HEX: u8 = b'<';
pub const SIGIL_NULL: u8 = b'^';
pub const SIGIL_STRING: u8 = b'"';

/// Per-key state maintained during pass 1.
#[derive(Debug, Default, Clone)]
pub struct KeyState {
    pub seen_int: bool,
    pub seen_float: bool,
    pub seen_bool: bool,
    pub seen_time: bool,
    pub seen_binary_hex: bool,
    pub seen_string: bool,
    pub seen_null: bool,
    pub total_records_seen_in: usize,
    /// Records in which this key was present (non-null).
    pub present_count: usize,
    /// Sigil from the very first non-null observation (for `FirstWins` policy).
    pub first_sigil: Option<u8>,
}

impl KeyState {
    /// Classify a raw string observation and merge into `self`.
    pub fn observe(&mut self, raw: &str) {
        self.total_records_seen_in += 1;
        if raw.is_empty() {
            self.seen_null = true;
            return;
        }
        self.present_count += 1;

        // int: parses as i64. Fires first so 0/1 stay int, not bool.
        if raw.parse::<i64>().is_ok() {
            self.seen_int = true;
            self.first_sigil.get_or_insert(SIGIL_INT);
            return;
        }
        // float: parses as f64 (and is not a pure int)
        if raw.parse::<f64>().is_ok() {
            self.seen_float = true;
            self.first_sigil.get_or_insert(SIGIL_FLOAT);
            return;
        }
        // bool: exactly true/false
        if raw == "true" || raw == "false" {
            self.seen_bool = true;
            self.first_sigil.get_or_insert(SIGIL_BOOL);
            return;
        }
        // time: contains '-' or 'T' and passes basic date/datetime heuristic
        if is_time_like(raw) {
            self.seen_time = true;
            self.first_sigil.get_or_insert(SIGIL_TIME);
            return;
        }
        // hex: length ≥ 16, even, all hex chars
        if is_hex_like(raw) {
            self.seen_binary_hex = true;
            self.first_sigil.get_or_insert(SIGIL_HEX);
            return;
        }
        self.seen_string = true;
        self.first_sigil.get_or_insert(SIGIL_STRING);
    }

    /// Collapse accumulated flags to a single sigil byte per plan priority.
    pub fn resolve_sigil(&self, policy: ConflictPolicy) -> Result<u8> {
        // Count how many distinct (non-null) types were observed.
        // String is itself a type for conflict-detection purposes.
        let type_count = [
            self.seen_int,
            self.seen_float,
            self.seen_bool,
            self.seen_time,
            self.seen_binary_hex,
            self.seen_string,
        ]
        .iter()
        .filter(|&&b| b)
        .count();

        if type_count > 1 {
            return match policy {
                ConflictPolicy::Error => Err(NxsError::ConvertSchemaConflict(
                    "mixed types observed for key".into(),
                )),
                ConflictPolicy::CoerceString => Ok(SIGIL_STRING),
                ConflictPolicy::FirstWins => {
                    // Use the sigil from the very first non-null observation.
                    Ok(self.first_sigil.unwrap_or(SIGIL_STRING))
                }
            };
        }

        // Single type — no conflict.
        if self.seen_string {
            return Ok(SIGIL_STRING);
        }

        if self.seen_int {
            return Ok(SIGIL_INT);
        }
        if self.seen_float {
            return Ok(SIGIL_FLOAT);
        }
        if self.seen_bool {
            return Ok(SIGIL_BOOL);
        }
        if self.seen_time {
            return Ok(SIGIL_TIME);
        }
        if self.seen_binary_hex {
            return Ok(SIGIL_HEX);
        }
        // All null/missing
        Ok(SIGIL_NULL)
    }
}

fn is_time_like(s: &str) -> bool {
    if s.len() < 8 {
        return false;
    }
    let has_sep = s.contains('-') || s.contains('T');
    if !has_sep {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '-' | ':' | 'T' | 'Z' | '+' | '.'))
}

fn is_hex_like(s: &str) -> bool {
    s.len() >= 16 && s.len() % 2 == 0 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Merge a per-record set of observations into the accumulator.
pub fn merge(acc: &mut InferredSchema, record: &[(String, String)]) {
    for (key, value) in record {
        let entry = acc.keys.iter().position(|k| &k.name == key);
        if let Some(i) = entry {
            if let Some(ks) = acc.key_states.get_mut(i) {
                ks.observe(value);
            }
        } else {
            let mut ks = KeyState::default();
            ks.observe(value);
            acc.keys.push(InferredKey {
                name: key.clone(),
                sigil: 0,
                optional: false,
                list_of: None,
            });
            acc.key_states.push(ks);
        }
    }
    acc.total_records += 1;
}

/// Freeze the accumulator into a schema ready to drive `NxsWriter`.
pub fn finalize(mut acc: InferredSchema, policy: ConflictPolicy) -> Result<InferredSchema> {
    for (key, state) in acc.keys.iter_mut().zip(acc.key_states.iter()) {
        key.sigil = state.resolve_sigil(policy)?;
        key.optional = state.present_count < acc.total_records;
    }
    Ok(acc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::ConflictPolicy;

    fn observe_all(values: &[&str]) -> KeyState {
        let mut ks = KeyState::default();
        for v in values {
            ks.observe(v);
        }
        ks
    }

    #[test]
    fn test_infer_priority_order() {
        // 1. int only
        let ks = observe_all(&["1", "2", "3"]);
        assert_eq!(ks.resolve_sigil(ConflictPolicy::Error).unwrap(), SIGIL_INT);

        // 2. int + float → two distinct types → conflict → CoerceString gives string
        let ks = observe_all(&["1", "2.5"]);
        assert_eq!(
            ks.resolve_sigil(ConflictPolicy::CoerceString).unwrap(),
            SIGIL_STRING
        );

        // 3. bool only
        let ks = observe_all(&["true", "false", "true"]);
        assert_eq!(ks.resolve_sigil(ConflictPolicy::Error).unwrap(), SIGIL_BOOL);

        // 4. bool + int → conflict; Error policy → Err
        let ks = observe_all(&["true", "0"]);
        // "0" parses as int, "true" parses as bool → two types
        assert!(ks.resolve_sigil(ConflictPolicy::Error).is_err());

        // 5. time only (ISO date)
        let ks = observe_all(&["2026-04-30", "2025-01-01"]);
        assert_eq!(ks.resolve_sigil(ConflictPolicy::Error).unwrap(), SIGIL_TIME);

        // 6. hex only (length ≥ 16, even, all hex)
        let ks = observe_all(&["deadbeefcafe0001", "0123456789abcdef"]);
        assert_eq!(ks.resolve_sigil(ConflictPolicy::Error).unwrap(), SIGIL_HEX);

        // 7. mixed int + string → string (with CoerceString)
        let ks = observe_all(&["1", "hello"]);
        assert_eq!(
            ks.resolve_sigil(ConflictPolicy::CoerceString).unwrap(),
            SIGIL_STRING
        );

        // 8. all null/missing → null sigil
        let ks = observe_all(&["", ""]);
        assert_eq!(ks.resolve_sigil(ConflictPolicy::Error).unwrap(), SIGIL_NULL);
    }

    #[test]
    fn test_infer_missing_keys_marked_optional() {
        let mut acc = InferredSchema::default();
        // Record 1: has "email"
        merge(&mut acc, &[("email".into(), "a@b.com".into())]);
        // Record 2: does NOT have "email" — advance total without adding key
        acc.total_records += 1;

        let schema = finalize(acc, ConflictPolicy::Error).unwrap();
        let email = schema.keys.iter().find(|k| k.name == "email").unwrap();
        assert!(email.optional, "key absent in one record must be optional");
    }

    #[test]
    fn test_infer_on_conflict_coerce_string() {
        let mut ks = KeyState::default();
        ks.observe("1"); // int
        ks.observe("hello"); // string
        let sigil = ks.resolve_sigil(ConflictPolicy::CoerceString).unwrap();
        assert_eq!(sigil, SIGIL_STRING);
    }

    #[test]
    fn test_infer_on_conflict_error() {
        let mut ks = KeyState::default();
        ks.observe("1"); // int
        ks.observe("hello"); // string
        let result = ks.resolve_sigil(ConflictPolicy::Error);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NxsError::ConvertSchemaConflict(_)
        ));
    }

    #[test]
    fn test_infer_first_wins_returns_first_observed_sigil() {
        // int first, then string → first_sigil = int
        let mut ks = KeyState::default();
        ks.observe("1"); // int → first_sigil = =
        ks.observe("hello"); // string → conflict
        assert_eq!(
            ks.resolve_sigil(ConflictPolicy::FirstWins).unwrap(),
            SIGIL_INT,
            "FirstWins: first-seen type (int) must win"
        );

        // string first, then int → first_sigil = string
        let mut ks2 = KeyState::default();
        ks2.observe("hello"); // string → first_sigil = "
        ks2.observe("1"); // int → conflict
        assert_eq!(
            ks2.resolve_sigil(ConflictPolicy::FirstWins).unwrap(),
            SIGIL_STRING,
            "FirstWins: first-seen type (string) must win"
        );

        // null then non-null: first_sigil must not be set by the null observation
        let mut ks3 = KeyState::default();
        ks3.observe(""); // null → first_sigil stays None
        ks3.observe("42"); // int → first_sigil = =
        ks3.observe("abc"); // string → conflict
        assert_eq!(
            ks3.resolve_sigil(ConflictPolicy::FirstWins).unwrap(),
            SIGIL_INT,
            "FirstWins: null observations must not pollute first_sigil"
        );
    }
}
