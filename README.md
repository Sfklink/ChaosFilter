# ChaosFilter

---

<<<<<<< HEAD
## Prequisites
1. stable rust toolchains: `rustup toolchain install stable`
2. nightly rust toolchains: `rustup toolchain install nightly --component rust-src`
3. (if cross-compiling) rustup target: `rustup target add ${ARCH}-unknown-linux-musl`
4. (if cross-compiling) LLVM: (e.g.) `brew install llvm` (on macOS)
5. (if cross-compiling) C toolchain: (e.g.) [`brew install filosottile/musl-cross/musl-cross`](https://github.com/FiloSottile/homebrew-musl-cross) (on macOS)
6. bpf-linker: `cargo install bpf-linker` (`--no-default-features` on macOS)

## Usage
```shell
Usage: chaosfilter <COMMAND>

Commands:
  status  Show current qdisc state for the interface
  update  Update qdisc
  reset   Restore interface (delete root qdisc + clsact)
  net     Run network chaos. Optionally scope to a cgroup (via tc classifier eBPF)
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

To see the arguments for each command, run the following:
```shell
chaosfilter <COMMAND> --help
```

Example:
```shell
chaosfilter net --help
```
=======

## Prerequisites

- Linux system with `tc` (iproute2)
- Root privileges (for qdisc manipulation)
- Rust toolchain:
  ```bash
  rustup toolchain install stable
Note that editing the qdisc requires running commands at root.

## Usage
Mode 1: Config File.
Run from a pre-existing *.toml file.

Example (Validate config):
```cargo run -- run -c chaosfilter.toml
```
Example (Run Config):
```
cargo run -p chaosfilter_cli -- run -c chaosfilter.toml
cargo run -p chaosfilter_cli -- validate -c chaosfilter.toml

```

chaosfilter.toml:
```name = "netem-test"

[targets]
cgroup = "system.slice"
iface = "enp34s0"

[schedule]
duration_ms = 20000

[features]
load_ebpf = false

[injectors.qdisc_netem]
enabled = true
delay_ms = 100
loss_percent = 50
```

Mode 2: Inline flags. 

Usage: 
```
chaosfilter run <COMMAND>

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
```
cargo build -p chaosfilter_cli
sudo target/debug/chaosfilter_cli run --iface enp34s0 --duration-ms 5000 --netem-delay-ms 50 --netem-loss-percent 0.2
```

>>>>>>> fc383c0 (Added command line option while retaining config file functionality.  Updated README.md to reflect this.)

---

## Build & Run
Use `cargo build`, `cargo check`, `cargo run`, etc. as normal. Build and run your program with:

<<<<<<< HEAD
=======

>>>>>>> fc383c0 (Added command line option while retaining config file functionality.  Updated README.md to reflect this.)
```shell
cargo build
cargo run -- <args>
```

Example:
```shell
cargo build
cargo run -- net --cgroup system.slice/sshd.service --iface enp5s0 --latency 300ms --loss 10% --duration 20s
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
chaosfilter --cgroup chaos-test <args>
<<<<<<< HEAD
```
=======
```

Program can also be pointed a .toml file to act as config, eg.:
```
cargo build -p chaosfilter_cli
target/debug/chaosfilter_cli run -c chaosfilter.toml
```


>>>>>>> fc383c0 (Added command line option while retaining config file functionality.  Updated README.md to reflect this.)
