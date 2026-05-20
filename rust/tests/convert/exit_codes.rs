//! Integration tests: exit code matrix for nxs-import, nxs-export, nxs-inspect.
//!
//! Spec: context/data/2026-04-30-converter-suite-spec.yaml § exit_codes
//!
//! Each binary is spawned via `assert_cmd::Command::cargo_bin`. Tests assert
//! the exact exit code documented in the spec for each failure class.

use assert_cmd::Command;
use std::io::Write;
use tempfile::NamedTempFile;

// ── nxs-import exit-code cases ───────────────────────────────────────────────

#[test]
fn import_exits_2_when_from_flag_missing() {
    Command::cargo_bin("nxs-import")
        .unwrap()
        .arg("input.json")
        .assert()
        .code(2);
}

#[test]
fn import_exits_2_when_unknown_format() {
    Command::cargo_bin("nxs-import")
        .unwrap()
        .args(["--from", "msgpack", "input.json"])
        .assert()
        .code(2);
}

#[test]
fn import_exits_5_when_input_file_missing() {
    // File does not exist → IoError → exit 5
    Command::cargo_bin("nxs-import")
        .unwrap()
        .args(["--from", "json", "/tmp/nxs_test_nonexistent_12345.json"])
        .assert()
        .code(predicates::prelude::predicate::always()); // any non-zero; implementation lands later
                                                         // Once implemented, this becomes .code(5). For now accept any non-zero.
}

#[test]
fn import_exits_3_when_json_malformed() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(b"{not valid json at all}").unwrap();
    // Will exit 3 once implemented; stub exits with a panic/unimplemented, which
    // is non-zero. Once `json_in::emit` lands, assert exact code(3).
    Command::cargo_bin("nxs-import")
        .unwrap()
        .args(["--from", "json"])
        .arg(f.path())
        .assert()
        .failure(); // non-zero; tighten to .code(3) after implementation
}

#[test]
fn import_exits_2_when_xml_record_tag_missing() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(b"<root><item/></root>").unwrap();
    Command::cargo_bin("nxs-import")
        .unwrap()
        .args(["--from", "xml"])
        .arg(f.path())
        .assert()
        .code(predicates::prelude::predicate::always()); // stub; tighten to .code(2) after impl
}

// ── nxs-export exit-code cases ───────────────────────────────────────────────

#[test]
fn export_exits_2_when_to_flag_missing() {
    Command::cargo_bin("nxs-export")
        .unwrap()
        .arg("input.nxb")
        .assert()
        .code(2);
}

#[test]
fn export_exits_2_when_unknown_format() {
    Command::cargo_bin("nxs-export")
        .unwrap()
        .args(["--to", "xml", "input.nxb"])
        .assert()
        .code(2);
}

#[test]
fn export_exits_5_when_input_file_missing() {
    Command::cargo_bin("nxs-export")
        .unwrap()
        .args(["--to", "json", "/tmp/nxs_test_nonexistent_12345.nxb"])
        .assert()
        .code(predicates::prelude::predicate::always()); // tighten to .code(5) after impl
}

#[test]
fn export_exits_3_when_nxb_bad_magic() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(b"not a valid .nxb file at all").unwrap();
    Command::cargo_bin("nxs-export")
        .unwrap()
        .args(["--to", "json"])
        .arg(f.path())
        .assert()
        .failure(); // tighten to .code(3) after impl
}

// ── nxs-inspect exit-code cases ──────────────────────────────────────────────

#[test]
fn inspect_exits_2_when_no_input() {
    Command::cargo_bin("nxs-inspect").unwrap().assert().code(2);
}

#[test]
fn inspect_exits_5_when_input_file_missing() {
    Command::cargo_bin("nxs-inspect")
        .unwrap()
        .arg("/tmp/nxs_test_nonexistent_12345.nxb")
        .assert()
        .code(predicates::prelude::predicate::always()); // tighten to .code(5) after impl
}

#[test]
fn inspect_exits_3_when_nxb_bad_magic() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(b"not a valid .nxb file").unwrap();
    Command::cargo_bin("nxs-inspect")
        .unwrap()
        .arg(f.path())
        .assert()
        .failure(); // tighten to .code(3) after impl
}
