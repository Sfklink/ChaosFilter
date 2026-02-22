# ChaosFilter

---

## Requirements


## Usage
To view usage:
```bash
cargo run -- --help
```
### Mode 1: Config File.

Run from a pre-existing *.toml file.

Example (Validate config):
```bash
cargo run -- validate -c chaosfilter.toml
```

`chaosfilter.toml`:
```toml
name = "netem-test"

[targets]
cgroup = "system.slice"
iface = "enp34s0"

[schedule]
duration_s = 20

[features]
load_ebpf = false

[injectors.qdisc_netem]
enabled = true
delay_ms = 100
loss_percent = 50
```

Example (Run Config):
```bash
cargo run -- chaos -c chaosfilter.toml
```

Example:
```bash
cargo build
sudo target/debug/chaosfilter_cli run --iface enp34s0 --duration-ms 5000 --netem-delay-ms 50 --netem-loss-percent 0.2
```

---

## Build & Run

If you are using mode 1, use `cargo build`, `cargo check`, `cargo run`, etc. as normal. Build and run your program with:
```shell
cargo build
cargo run -- <args>
```

Example:
```shell
cargo build
cargo run -- chaos --cgroup system.slice/sshd.service --iface enp5s0 --latency 300ms --loss 10% --duration 20s
```

If a cgroup is not readily available, you can create one yourself named `chaos-test`:
```shell
systemd-run --user --scope -p "Delegate=yes" --unit=chaos-test bash
```

Verify with the following:
```shell
cat /proc/self/cgroup
ping -c 3 8.8.8.8
```

and use it in chaos filter run by doing:
```shell
chaosfilter run --cgroup chaos-test <args>
```

Program can also be pointed a .toml file to act as config, eg.:
```bash
cargo build
target/debug/chaosfilter_cli run -c chaosfilter.toml
```


## Development

To add a new module in the controller directory (in this example, Storage), navigate to the Injector struct in cli.rs.

```
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



and then create your ```StorageConfig``` struct in cli.rs.

```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageConfig &#123;
    pub enabled: bool,
    pub targetdrive: Option<String>,
    pub var1: u32,
    pub var2: Option<String>,
    pub var3: i32,
    ...
}
```

Once added, ```StorageConfig``` will be included in the ```Plan``` struct, and have access to its members.
Ensure that ```use crate::cli::Plan``` is included in your module.

To maintain a level of parity between modules, ensure that all domain-specific logic (Network, Cgroups, Storage) is
self-contained within each module.  This will aid future developers in maintaining the software's architecture.
## Documentation

To access documentation, run:
```bash
cargo doc --open
```



## Cross-compiling on macOS

Cross compilation should work on both Intel and Apple Silicon Macs.
I do not understand this section.  Why is this here?


```shell
CC=${ARCH}-linux-musl-gcc cargo build --package chaosfilter --release \
  --target=${ARCH}-unknown-linux-musl \
  --config=target.${ARCH}-unknown-linux-musl.linker=\"${ARCH}-linux-musl-gcc\"
```
The cross-compiled program `target/${ARCH}-unknown-linux-musl/release/chaosfilter` can be
copied to a Linux server or VM and run there.
