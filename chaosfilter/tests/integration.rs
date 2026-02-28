use tempfile::NamedTempFile;
use std::{io::Write, process::{Command, Output}};

fn have_vmtest() -> bool {
    Command::new("sh")
        .args(["-lc", "command -v vmtest >/dev/null 2>&1"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_vmtest_script(chaosfilter_toml: String) -> Output {
    let toml = format!(r#"
        [[target]]
        name = "chaosfilter help"
        kernel = "chaosfilter/tests/kernels/bzImage-v6.2-default"
        command = '''/bin/bash -lc '
        ip link set eth0 up
        ip -o link || true

        cd /mnt/vmtest
        cat > /tmp/chaosfilter.toml <<EOF
        {chaosfilter_toml}
        EOF

        cargo build
        ./target/debug/chaosfilter validate --config /tmp/chaosfilter.toml
        '
        '''

        [target.vm]
        extra_args = ["-nic", "user,model=virtio-net-pci"]
    "#);

    let mut f = NamedTempFile::new().expect("Could not create temp vmtest config");
    write!(f, "{toml}").expect("Could not create vmtest config");

    Command::new("vmtest")
        .args(["--config", f.path().to_str().unwrap()])
        .output()
        .expect("failed to execute vmtest")
}

fn create_chaosfilter_config_toml(iface: &str) -> String {
    format!(r#"
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
    "#)
}

#[test]
fn validate_ok() {
    if !have_vmtest() {
        eprintln!("Error: vmtest not installed");
        return;
    }

    let chaosfilter_toml = create_chaosfilter_config_toml("eth0");

    let out = run_vmtest_script(chaosfilter_toml);

    // Output for Debugging
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        stdout.contains("Config OK"),
        "Expected 'Config OK' but got:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}" 
    );
}

#[test]
fn validate_iface_invalid() {
    if !have_vmtest() {
        eprintln!("skipping: vmtest not installed");
        return;
    }

    let chaosfilter_toml = create_chaosfilter_config_toml("piss");

    let out = run_vmtest_script(chaosfilter_toml);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "expected failure but succeeded and got:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}\n"
    );

    assert!(
        stderr.contains("network interface not found") || stdout.contains("network interface not found"),
        "expected 'network interface not found' in output but got:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}\n"
    );
}

#[test]
fn vm_validate_fail_empty_iface() {
    if !have_vmtest() {
        eprintln!("skipping: vmtest not installed");
        return;
    }

    let chaosfilter_toml = create_chaosfilter_config_toml("");

    let out = run_vmtest_script(chaosfilter_toml);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !out.status.success(),
        "expected failure but succeeded and got:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}\n"
    );
}