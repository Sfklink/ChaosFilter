use std::{env, fs, path::PathBuf};

fn main() {
    // Re-run if eBPF sources or manifest change
    println!("cargo:rerun-if-changed=../ebpf/src");
    println!("cargo:rerun-if-changed=../ebpf/Cargo.toml");

    // Re-run if the built eBPF artifact changes (this is the key fix)
    println!("cargo:rerun-if-changed=../target/bpfel-unknown-none/release/libchaosfilter_ebpf.so");

    let ebpf_obj = PathBuf::from("../target/bpfel-unknown-none/release/libchaosfilter_ebpf.so");

    if !ebpf_obj.exists() {
        panic!(
            "eBPF artifact not found at {:?}. Build ebpf first with: cargo +nightly build -p chaosfilter-ebpf --target bpfel-unknown-none --release",
            ebpf_obj
        );
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_obj = out_dir.join("chaosfilter-ebpf.o"); // name doesn't matter; it's still ELF bytes

    fs::copy(&ebpf_obj, &out_obj).expect("failed to copy eBPF artifact into OUT_DIR");
}
