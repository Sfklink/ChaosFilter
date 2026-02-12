# ChaosFilter

---

## Prerequisites

- Linux system with `tc` (iproute2)
- Root privileges (for qdisc manipulation)
- Rust toolchain:
```bash
rustup toolchain install stable
```
Note that editing the qdisc requires running commands at root.

## Usage
To view usage:
```bash
cargo run -- --help
```

#### Note:
The `--` is not required when running commands, but it does enable autocomplete w/ tab.

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

### Mode 2: Inline flags. 

Usage: 
```bash
chaosfilter chaos <COMMAND>

--iface <iface>
Network interface to apply chaos to (required for inline mode)

--duration-ms <ms>
Duration to hold chaos before revert (default: 5000)

--netem-enabled <bool>
Enable/disable qdisc netem (default: true)

--netem-delay-ms <ms>
Artificial latency in milliseconds

--netem-loss-percent <percent>
Packet loss percentage (e.g. 0.2)

--cgroup <name>
Optional cgroup (relative to /sys/fs/cgroup)
(currently validated only; scoping via eBPF is future work)

--load-ebpf
Enable eBPF loading (feature-gated, optional)
```

Example:
```bash
cargo build
sudo target/debug/chaosfilter_cli run --iface enp34s0 --duration-ms 5000 --netem-delay-ms 50 --netem-loss-percent 0.2
```

### Mode 3: Menu GUI

```bash
cargo run -- menu
```

Output:
```bash
What system would you like to test?
1) Network Stack
2) Disk I/O
3) CPU / Scheduling
4) Exit
>
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

## Documentation

To access documentation, run:
```bash
cargo doc --open
```

## Cross-compiling on macOS

Cross compilation should work on both Intel and Apple Silicon Macs.

```shell
CC=${ARCH}-linux-musl-gcc cargo build --package chaosfilter --release \
  --target=${ARCH}-unknown-linux-musl \
  --config=target.${ARCH}-unknown-linux-musl.linker=\"${ARCH}-linux-musl-gcc\"
```
The cross-compiled program `target/${ARCH}-unknown-linux-musl/release/chaosfilter` can be
copied to a Linux server or VM and run there.

## License

With the exception of eBPF code, chaosfilter is distributed under the terms
of either the [MIT license] or the [Apache License] (version 2.0), at your
option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

### eBPF

All eBPF code is distributed under either the terms of the
[GNU General Public License, Version 2] or the [MIT license], at your
option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project by you, as defined in the GPL-2 license, shall be
dual licensed as above, without any additional terms or conditions.

[Apache license]: LICENSE-APACHE
[MIT license]: LICENSE-MIT
[GNU General Public License, Version 2]: LICENSE-GPL2
