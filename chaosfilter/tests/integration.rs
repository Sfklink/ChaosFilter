//! # Integration Tests
//!
//! This module contains integration tests for the `chaosfilter` CLI.
//! It uses `assert_cmd` and `tempfile` to execute the binary and verify its behavior
//! against various configuration scenarios.

use assert_cmd::{assert::Assert, cargo};
use predicates::prelude::predicate;
use std::io::Write;
use tempfile::NamedTempFile;

/// Helper to determine the default network interface.
///
/// This is used to generate valid configuration files for testing.
fn get_default_iface() -> Option<String> {
    let output = std::process::Command::new("ip")
        .args(["route", "get", "8.8.8.8"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    stdout
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "dev")
        .map(|w| w[1].to_string())
}

/// Helper to create a sample TOML configuration string.
fn create_chaosfilter_toml(iface: &str) -> String {
    return format!(
        r#"
name = "test"
[targets]
iface = "{iface}"

[schedule]
duration_s = 10

[injectors.network_config]
enabled = true
target_iface = "{iface}"
delay_ms = 100
loss_percent = 50.0

[injectors.memory_config]
enabled = false
target_pid = 1234
move_pid = true
enable = ["cpu", "memory"]
cpu_max = "20000 100000"
mem_max = "1G"
"#
    );
}

/// Helper to print the stdout and stderr of a completed command.
fn print_output(cmd: Assert) {
    eprint!(
        "----- stdout -----\n{}",
        String::from_utf8_lossy(&cmd.get_output().stdout)
    );
    eprintln!(
        "----- stderr -----\n{}",
        String::from_utf8_lossy(&cmd.get_output().stderr)
    );
}

/// Verifies that a valid configuration file passes validation.
#[test]
fn validate_ok() {
    let iface = get_default_iface().unwrap();
    let toml = create_chaosfilter_toml(&iface);

    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{toml}").unwrap();

    let cmd = cargo::cargo_bin_cmd!("chaosfilter")
        .args(["validate", "--config", file.path().to_str().unwrap()])
        .assert()
        .success();

    print_output(cmd);
}

/// Verifies that an invalid configuration file (missing fields) fails validation.
#[test]
fn validate_fail() {
    let toml = format!(
        r#"
name = "test"

[targets]
iface = ""
cgroup = "test"
"#
    );

    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{toml}").unwrap();

    let cmd = cargo::cargo_bin_cmd!("chaosfilter")
        .args(["validate", "--config", file.path().to_str().unwrap()])
        .assert()
        .failure();

    print_output(cmd);
}

/// Verifies that validation fails if the specified network interface does not exist.
#[test]
fn validate_iface_invalid_iface() {
    let toml = create_chaosfilter_toml("test");

    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{toml}").unwrap();

    let cmd = cargo::cargo_bin_cmd!("chaosfilter")
        .args(["validate", "--config", file.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("network interface not found"));

    print_output(cmd);
}

/// Verifies that a chaos plan can be successfully executed.
///
/// Note: This test may require root privileges or specific capabilities to succeed.
#[test]
fn run_chaos_plan() {
    let iface = get_default_iface().unwrap();
    let toml = create_chaosfilter_toml(&iface);

    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{toml}").unwrap();

    let cmd = cargo::cargo_bin_cmd!("chaosfilter")
        .args(["chaos", "--config", file.path().to_str().unwrap()])
        .assert()
        .success();

    print_output(cmd);
}

/// Verifies that running a chaos plan fails if the interface is invalid.
#[test]
fn run_chaos_plan_invalid_iface() {
    let toml = create_chaosfilter_toml("test");

    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{toml}").unwrap();

    let cmd = cargo::cargo_bin_cmd!("chaosfilter")
        .args(["chaos", "--config", file.path().to_str().unwrap()])
        .assert()
        .failure();

    print_output(cmd);
}

/// Verifies that the `init` command generates a valid configuration file.
#[test]
fn init_generates_config() {
    let tempdir = tempfile::tempdir().unwrap();
    let config_path = tempdir.path().join("cf-config.toml");

    let cmd = cargo::cargo_bin_cmd!("chaosfilter")
        .args(["init", "--pid", "1234", "--iface", "lo"])
        .current_dir(tempdir.path())
        .assert()
        .success();

    print_output(cmd);
    assert!(config_path.exists());
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("target_pid = \"1234\""));
    assert!(content.contains("target_iface = \"lo\""));
}

/// Verifies that `init` fails if the config file already exists and --force is not used.
#[test]
fn init_fails_if_exists() {
    let tempdir = tempfile::tempdir().unwrap();
    let config_path = tempdir.path().join("cf-config.toml");
    std::fs::write(&config_path, "old").unwrap();

    let cmd = cargo::cargo_bin_cmd!("chaosfilter")
        .args(["init"])
        .current_dir(tempdir.path())
        .assert()
        .failure();

    print_output(cmd);
}

/// Verifies that `init` succeeds with --force even if the file exists.
#[test]
fn init_succeeds_with_force() {
    let tempdir = tempfile::tempdir().unwrap();
    let config_path = tempdir.path().join("cf-config.toml");
    std::fs::write(&config_path, "old").unwrap();

    let cmd = cargo::cargo_bin_cmd!("chaosfilter")
        .args(["init", "--force"])
        .current_dir(tempdir.path())
        .assert()
        .success();

    print_output(cmd);
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("name = \"example-plan\""));
}

/// Verifies that `validate` fails with a non-existent config file.
#[test]
fn validate_missing_file() {
    let cmd = cargo::cargo_bin_cmd!("chaosfilter")
        .args(["validate", "--config", "/tmp/does_not_exist_12345.toml"])
        .assert()
        .failure();

    print_output(cmd);
}

/// Verifies that `validate` fails with invalid TOML.
#[test]
fn validate_invalid_toml() {
    let mut file = NamedTempFile::new().unwrap();
    write!(file, "this is not toml").unwrap();

    let cmd = cargo::cargo_bin_cmd!("chaosfilter")
        .args(["validate", "--config", file.path().to_str().unwrap()])
        .assert()
        .failure();

    print_output(cmd);
}

