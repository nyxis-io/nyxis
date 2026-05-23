#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = nxs::decoder::decode(data);
    if let Ok(reader) = nxs::query::Reader::new(data) {
        let n = reader.record_count().min(32);
        for i in 0..n {
            if let Some(rec) = reader.record(i) {
                for key in reader.keys() {
                    let _ = rec.get_i64(key);
                    let _ = rec.get_f64(key);
                    let _ = rec.get_bool(key);
                    let _ = rec.get_str(key);
                }
            }
        }
    }
});
