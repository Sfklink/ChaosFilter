use assert_cmd::cargo;
use predicates::prelude::predicate;
use tempfile::NamedTempFile;
use std::io::Write;

fn get_default_iface() -> Option<String> {
    let output = std::process::Command::new("ip")
        .args(["route", "get", "8.8.8.8"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Look for: "dev <iface>"
    stdout
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "dev")
        .map(|w| w[1].to_string())
}

#[test]
fn validate_ok() {
    let iface = get_default_iface().unwrap();
    let toml = format!(r#"
        name = "test"
        [targets]
        iface = "{iface}"
        cgroup = "test"

        [schedule]
        duration_s = 1

        [injectors.network_config]
        enabled = false

        [injectors.memory_config]
        enabled = false
        "#);

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