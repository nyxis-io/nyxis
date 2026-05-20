//! End-to-end integration tests: JSON roundtrip, CSV roundtrip, 10k perf smoke.

use assert_cmd::Command;
use std::io::Write;
use tempfile::NamedTempFile;

// ── JSON roundtrip: import → export → parse → deep-equal ─────────────────────

#[test]
fn e2e_json_roundtrip_value_equivalent() {
    // Build a 5-record JSON input.
    let original: Vec<serde_json::Value> = (0u32..5)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "name": format!("user_{i}"),
                "active": true,
                "score": i as f64 * 1.5
            })
        })
        .collect();
    let input_json = serde_json::to_vec(&original).unwrap();

    // nxs-import --from json <input> → .nxb tempfile
    let mut json_file = NamedTempFile::new().unwrap();
    json_file.write_all(&input_json).unwrap();
    let nxb_file = NamedTempFile::new().unwrap();

    Command::cargo_bin("nxs-import")
        .unwrap()
        .args(["--from", "json"])
        .arg(json_file.path())
        .arg(nxb_file.path())
        .assert()
        .success();

    // nxs-export --to json <.nxb> → JSON tempfile
    let out_json_file = NamedTempFile::new().unwrap();
    Command::cargo_bin("nxs-export")
        .unwrap()
        .args(["--to", "json"])
        .arg(nxb_file.path())
        .arg(out_json_file.path())
        .assert()
        .success();

    // Parse both, compare as sorted-key Value maps (order-independent).
    let exported_bytes = std::fs::read(out_json_file.path()).unwrap();
    let exported: Vec<serde_json::Value> =
        serde_json::from_slice(exported_bytes.trim_ascii()).unwrap();

    assert_eq!(
        exported.len(),
        original.len(),
        "exported record count must match"
    );
    for (orig, exp) in original.iter().zip(exported.iter()) {
        let orig_obj = orig.as_object().unwrap();
        let exp_obj = exp.as_object().unwrap();
        for (k, v) in orig_obj {
            let ev = exp_obj
                .get(k)
                .unwrap_or_else(|| panic!("key {k} missing in exported record"));
            // Numeric equivalence (float comparison with tolerance for f64).
            match (v, ev) {
                (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                    let af = a.as_f64().unwrap();
                    let bf = b.as_f64().unwrap();
                    assert!(
                        (af - bf).abs() < 1e-9_f64.max(af.abs() * 1e-9),
                        "numeric mismatch for key {k}: {af} vs {bf}"
                    );
                }
                _ => assert_eq!(v, ev, "value mismatch for key {k}"),
            }
        }
    }
}

// ── CSV roundtrip: import → export → parse → value-equivalent ────────────────

#[test]
fn e2e_csv_roundtrip_value_equivalent() {
    let csv_input = b"id,name,active\n1,alice,true\n2,bob,false\n3,carol,true\n" as &[u8];

    let mut csv_file = NamedTempFile::new().unwrap();
    csv_file.write_all(csv_input).unwrap();
    let nxb_file = NamedTempFile::new().unwrap();

    Command::cargo_bin("nxs-import")
        .unwrap()
        .args(["--from", "csv"])
        .arg(csv_file.path())
        .arg(nxb_file.path())
        .assert()
        .success();

    let out_csv_file = NamedTempFile::new().unwrap();
    Command::cargo_bin("nxs-export")
        .unwrap()
        .args(["--to", "csv"])
        .arg(nxb_file.path())
        .arg(out_csv_file.path())
        .assert()
        .success();

    let out_text = std::fs::read_to_string(out_csv_file.path()).unwrap();
    // Header must be present and all three keys appear.
    let header = out_text.lines().next().unwrap();
    assert!(header.contains("id"), "id missing from header");
    assert!(header.contains("name"), "name missing from header");
    assert!(header.contains("active"), "active missing from header");

    let data_lines: Vec<&str> = out_text.lines().skip(1).collect();
    assert_eq!(data_lines.len(), 3, "must have 3 data rows");

    // All original names and booleans must appear somewhere.
    assert!(out_text.contains("alice"));
    assert!(out_text.contains("bob"));
    assert!(out_text.contains("carol"));
    assert!(out_text.contains("true"));
    assert!(out_text.contains("false"));
}

// ── 10k perf smoke: import 10k-record JSON, advisory wall-clock < 1s ─────────

#[test]
fn e2e_10k_records_under_threshold() {
    // Only enforced on release builds; debug is expected to be slower.
    let profile = std::env::var("CARGO_PROFILE").unwrap_or_default();
    let is_release =
        profile == "release" || std::env::var("NXS_PERF_SMOKE").unwrap_or_default() == "1";

    let records: Vec<serde_json::Value> = (0u32..10_000)
        .map(|i| serde_json::json!({"id": i, "name": format!("user_{i}")}))
        .collect();
    let input_json = serde_json::to_vec(&records).unwrap();

    let mut json_file = NamedTempFile::new().unwrap();
    json_file.write_all(&input_json).unwrap();
    let nxb_file = NamedTempFile::new().unwrap();

    let start = std::time::Instant::now();
    Command::cargo_bin("nxs-import")
        .unwrap()
        .args(["--from", "json"])
        .arg(json_file.path())
        .arg(nxb_file.path())
        .assert()
        .success();
    let elapsed = start.elapsed();

    if is_release {
        assert!(
            elapsed.as_secs_f64() < 1.0,
            "10k-record import took {elapsed:?} on release build; threshold is 1s (advisory)"
        );
    }
    // Even in debug, assert the output is valid.
    let out_bytes = std::fs::read(nxb_file.path()).unwrap();
    assert!(out_bytes.len() >= 4);
    assert_eq!(
        &out_bytes[0..4],
        &[0x42, 0x58, 0x59, 0x4E],
        "output must start with NYXB magic"
    );
}
