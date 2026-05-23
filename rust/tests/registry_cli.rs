//! CLI smoke tests for `nxs registry` (no live registryd required).

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn nxs_help_mentions_registry() {
    Command::cargo_bin("nxs")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("registry"));
}

#[test]
fn registry_help_lists_subcommands() {
    Command::cargo_bin("nxs")
        .unwrap()
        .args(["registry", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("push")
                .and(predicate::str::contains("list"))
                .and(predicate::str::contains("diff")),
        );
}

#[test]
fn registry_list_without_hash_prints_mvp_note() {
    Command::cargo_bin("nxs")
        .unwrap()
        .args(["registry", "list", "--server", "127.0.0.1:1"])
        .assert()
        .success()
        .stderr(predicate::str::contains("ListSchemas"));
}
