# ChaosFilter

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

To add a new module in the controller directory (in this example, Storage), navigate to the Injector struct in cli.rs.

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

To maintain a level of parity between modules, ensure that all domain-specific logic (Network, Cgroups, Storage) is self-contained within each module.  This will aid future developers in maintaining the software's architecture.

## Documentation

To access documentation, run:
```bash
cargo doc --open
```