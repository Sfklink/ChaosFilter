# ChaosFilter

---

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

---

## Build & Run
Use `cargo build`, `cargo check`, `cargo run`, etc. as normal. Build and run your program with:

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
```