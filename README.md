# ChaosFilter

---

## Environment Setup
Here is everything you need to do to go from a fresh Linux install to being able to run ChaosFilter.

NOTE: Secure boot can interfere with some eBPF behavior. Disable if needed.

### 1 System Packages
Start by updating the system first:
```bash
sudo apt update
sudo apt upgrade -y
```

These are all the system packages that are required:
```bash
sudo apt install -y \
build-essential \
clang \
llvm \
libelf-dev \
zlib1g-dev \
libclang-dev \
linux-headers-$(uname -r) \
pkg-config \
bpftool \
iproute2 \
iptables \
net-tools \
curl 
```

### 2 Install Rust
Install Rust with rustup:
```bash
curl https://sh.rustup.rs -sSf | sh
```

Select default installation (stable) then reload the shell and verify:
```bash
source $HOME/.cargo/env

rustc --version
cargo --version
```

### 3 Install BPF Targets for Rust
Add the BPF compilation target:
```bash
rustup target add bpfel-unknown-none
```

If using Aya build scripts (recommended) also install:
```bash
cargo install cargo-generate
```

### 4 Verify the Kernel Supports eBPF
Check BPF support you should see most BPF features marked as available:
```bash
bpftool feature
```

Check the cgroup version:
```bash
stat -fc %T /sys/fs/cgroup/
```
You should see:
```bash
cgroup2fs
```

### 5 Increase memlock Limit (Important for eBPF)
There are 2 method to do this a temporary one and a permanent one. The recommendation will depend on your use-case. If you plan to use ChaosFilter across multiple session go with the permanent method. Otherwise use the temporary method

For a temporary (current session only) increase:
```bash
ulimit -l unlimited
```

For a permanent increase:
```bash
sudo nano /etc/security/limits.conf
```
Add:
```code
soft memlock unlimited
hard memlock unlimited
```
Then reboot and verify:
```bash
ulimit -l
```
You should see:
```code
unlimited
```

### 6 Verification of Working 'tc'
Run:
```bash
tc qdisc show
```
You should see this or something similar:
```code
qdisc fq_codel 0: dev enpX root fercnt 2
```

---

## Usage
To view usage:
```bash
cargo build
cargo run -- --help
```

How to run from a pre-existing *.toml file.

Example (Build & Validate with config):
```bash
cargo build
cargo run -- validate -c, --config <path-to-toml-file>
```

Example Toml Config File:

Run ```chaosfilter --schema``` to see a sample config file with variable descriptors.

`chaosfilter.toml`:
```toml
name = "netem-test"

[targets]
iface = "enp5s0"
cgroup = "77500" # Optional

[schedule]
duration_s = 10

[injectors.network_config]
enabled = true
target_iface = "enp5s0"
delay_ms = 100
loss_percent = 50.0

[injectors.memory_config]
enabled = false
target_pid = 1234
move_pid = true
enable = ["cpu", "memory"]
cpu_max = "20000 100000"
mem_max = "1G"
```

Example (Build & Run with Config):
```bash
cargo build
cargo run -- chaos -c, --config chaosfilter.toml
```

---

## Development

To add a new module in the controller directory (in this example, ```StorageConfig```), navigate to the ```Injectors``` struct in cli.rs.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Injectors {
    #[serde(default)]
    pub network_config: NetworkConfig,
    #[serde(default)]
    pub memory_config: MemoryConfig,
    #[serde(default)]
    pub storage_config: StorageConfig,
}
```

and then create your `StorageConfig` struct in cli.rs.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageConfig {
    pub enabled: bool,
    pub targetdrive: Option<String>,
    pub var1: u32,
    pub var2: Option<String>,
    pub var3: i32,
    ...
}
```

Once added, `StorageConfig` will be included in the `Plan` struct, and have access to its members. Ensure that `use crate::cli::Plan` is included in your module.

To maintain a level of parity between modules, ensure that all domain-specific logic (Network, Memory, Storage) is self-contained within each module.  This will aid future developers in maintaining the software's architecture.

## Documentation

To access documentation, run:
```bash
cargo doc --open
```