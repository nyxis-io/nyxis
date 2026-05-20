#![no_main]
use libfuzzer_sys::fuzz_target;
use nxs::writer::{NxsWriter, Schema, Slot};

// Fuzz the round-trip: any sequence of slot writes must produce a .nxb
// that the decoder can parse without panicking.
fuzz_target!(|data: &[u8]| {
    if data.len() < 4 { return; }

    let n_keys = (data[0] as usize % 8) + 1;
    let n_records = (data[1] as usize % 16) + 1;

    // Build a small schema from n_keys synthetic names
    let key_names: Vec<String> = (0..n_keys).map(|i| format!("f{i}")).collect();
    let key_refs: Vec<&str> = key_names.iter().map(|s| s.as_str()).collect();
    let schema = Schema::new(&key_refs);
    let mut w = NxsWriter::new(&schema);

    let mut pos = 2usize;
    for _ in 0..n_records {
        w.begin_object();
        for slot_idx in 0..n_keys {
            if pos + 9 > data.len() { break; }
            let present = data[pos] & 1 == 1;
            pos += 1;
            if present {
                let val = i64::from_le_bytes(data[pos..pos+8].try_into().unwrap_or([0u8;8]));
                w.write_i64(Slot(slot_idx as u16), val);
                pos += 8;
            }
        }
        w.end_object();
    }

    let bytes = w.finish();
    // Must never panic
    let _ = nxs::decoder::decode(&bytes);
});
