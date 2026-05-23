//! Go writer → Rust reader roundtrip (requires `go` and nyxis-drivers checkout).

use std::path::PathBuf;
use std::process::Command;

fn go_driver_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../nyxis-drivers/go")
}

#[test]
fn rust_reads_go_produced_minimal_nxb() {
    let go_dir = go_driver_dir();
    if !go_dir.join("go.mod").is_file() {
        eprintln!("skip: {} not found", go_dir.display());
        return;
    }
    let out = tempfile::NamedTempFile::new().expect("temp");
    let status = Command::new("go")
        .current_dir(&go_dir)
        .env("NXS_GO_PRODUCER_OUT", out.path())
        .args([
            "test",
            "-run",
            "^TestWriteProducerFixture$",
            ".",
            "-count=1",
        ])
        .status()
        .expect("spawn go test");
    assert!(status.success(), "go test failed: {status:?}");

    let data = std::fs::read(out.path()).expect("read go nxb");
    let reader = nxs::query::Reader::new(&data).expect("Reader::new");
    assert_eq!(reader.record_count(), 1);
    let rec = reader.record(0).expect("record 0");
    assert_eq!(rec.get_i64("id"), Some(42));
    assert_eq!(rec.get_str("name"), Some("hello"));
    assert_eq!(rec.get_bool("active"), Some(true));
}
