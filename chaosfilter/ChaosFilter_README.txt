ChaosFilter Setup Guide
========================

ChaosFilter is a Rust + Aya based chaos-engineering project that uses eBPF to
observe and later inject controlled failures into system behavior. The project
is split into a userspace control application and an eBPF program built using Aya.

This setup guide is designed so the project can be built and run on any supported
Linux machine with minimal manual configuration.

--------------------------------------------------
1. Operating System Requirements
--------------------------------------------------

- Linux host (kernel 5.8+ recommended)
- eBPF support enabled in the kernel (CONFIG_BPF)

Note:
This project will not run on Windows or macOS directly.

--------------------------------------------------
2. Required System Packages
--------------------------------------------------

Ubuntu / Debian:
----------------
sudo apt update
sudo apt install -y clang libelf-dev bpftool

Fedora:
-------
sudo dnf install -y clang elfutils-libelf-devel bpftool

Arch Linux:
-----------
sudo pacman -S clang libelf bpftool

--------------------------------------------------
3. Rust Toolchain Installation
--------------------------------------------------

Install Rust using rustup (recommended):

curl https://sh.rustup.rs -sSf | sh
source $HOME/.cargo/env

This repository includes a rust-toolchain.toml file, which automatically
selects the correct Rust nightly version and target when building.

No manual Rust version selection is required.

--------------------------------------------------
4. Project Structure
--------------------------------------------------

chaosfilter/
├── control/    (Userspace Rust controller)
├── ebpf/       (eBPF programs using aya-ebpf)
├── xtask/      (Aya build helper for eBPF)
├── Cargo.toml  (Workspace root)
└── rust-toolchain.toml

--------------------------------------------------
5. Building the eBPF Program
--------------------------------------------------

The eBPF program must be built before the userspace controller.

From the repository root, run:

cargo run -p xtask

This command:
- Uses Rust’s internal LLVM
- Avoids system LLVM version conflicts
- Produces a valid eBPF ELF artifact

--------------------------------------------------
6. Building the Userspace Controller
--------------------------------------------------

After the eBPF program is built:

cargo build -p chaosfilter

--------------------------------------------------
7. Running ChaosFilter
--------------------------------------------------

Loading eBPF programs requires elevated privileges.

Run the controller with:

sudo ./target/debug/chaosfilter

--------------------------------------------------
8. Verifying eBPF Support (Optional)
--------------------------------------------------

You can verify kernel eBPF support with:

bpftool feature probe

If this command fails, your kernel does not support eBPF.

--------------------------------------------------
9. Common Issues
--------------------------------------------------

Permission errors:
- Ensure the program is run with sudo.

Kernel too old:
- Upgrade to a kernel version 5.8 or newer.

rlimit / memlock errors:
- Temporarily fix with:
  ulimit -l unlimited

--------------------------------------------------
10. Notes
--------------------------------------------------

- This project intentionally avoids system LLVM dependencies.
- All eBPF compilation is handled through Aya’s supported build path.
- No manual linker configuration is required.

--------------------------------------------------
11. Project Status
--------------------------------------------------

- eBPF environment setup: COMPLETE
- Chaos injection logic: IN PROGRESS
- qdisc-based network chaos: PLANNED

--------------------------------------------------
License
--------------------------------------------------

MIT (or update as appropriate)
