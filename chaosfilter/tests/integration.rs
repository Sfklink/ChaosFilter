use assert_cmd::cargo;
use predicates::prelude::predicate;
use tempfile::NamedTempFile;
use std::io::Write;

#[test]
fn validate_ok() {
    let toml = r#"
        name = "test"
        [targets]
        iface = "enp5s0"
        cgroup = "test"

        [schedule]
        duration_s = 1

        [injectors.network_config]
        enabled = false

        [injectors.memory_config]
        enabled = false
        "#;

    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{toml}").unwrap();

    cargo::cargo_bin_cmd!("chaosfilter")
        .args(["validate", "--config", file.path().to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn validate_fail() {
    let toml = r#"
        name = "test"

        [targets]
        iface = ""
        cgroup = "test"
        "#;

    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{toml}").unwrap();

    cargo::cargo_bin_cmd!("chaosfilter")
        .args(["validate", "--config", file.path().to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn validate_iface_invalid() {
        let toml = r#"
        name = "test"
        [targets]
        iface = "test"

        [schedule]
        duration_s = 1

        [injectors.network_config]
        enabled = false

        [injectors.memory_config]
        enabled = false
        "#;
    
    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{toml}").unwrap();

    cargo::cargo_bin_cmd!("chaosfilter")
        .args(["validate", "--config", file.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("network interface not found"));
}