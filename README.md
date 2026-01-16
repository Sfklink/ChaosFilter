# hello-world

## Prerequisites

1. stable rust toolchains: `rustup toolchain install stable`
2. nightly rust toolchains: `rustup toolchain install nightly --component rust-src`
3. (if cross-compiling) rustup target: `rustup target add ${ARCH}-unknown-linux-musl`
4. (if cross-compiling) LLVM: (e.g.) `brew install llvm` (on macOS)
5. (if cross-compiling) C toolchain: (e.g.) [`brew install filosottile/musl-cross/musl-cross`](https://github.com/FiloSottile/homebrew-musl-cross) (on macOS)
6. bpf-linker: `cargo install bpf-linker` (`--no-default-features` on macOS)

## Build & Run
```bash
cargo build
cargo run
```

This may not work at first glance, nor will you get any logs by doing this. Run this instead:
```bash
cargo build
RUST_LOG=info cargo run --config 'target."cfg(all())".runner="sudo -E"' --   --iface <YOUR-NET-INTERFACE-HERE>
```

To find your network interface, run the following:
```bash
ip a
```

Select your network. It will most likely say **BROADCAST, MULTICAST, etc.** if you are using wifi/ethernet.

Everytime you type ls it will print "Hello World"