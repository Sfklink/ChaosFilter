#![no_std]
#![no_main]

use aya_ebpf::{bindings::TC_ACT_PIPE, macros::classifier, programs::TcContext};
use aya_log_ebpf::info;

#[classifier]
pub fn chaosfilter(ctx: TcContext) -> i32 {
    match try_chaosfilter(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_chaosfilter(ctx: TcContext) -> Result<i32, i32> {
    info!(&ctx, "received a packet");
    Ok(TC_ACT_PIPE)
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(link_section = "license")]
#[unsafe(no_mangle)]
static LICENSE: [u8; 13] = *b"Dual MIT/GPL\0";

// pub fn load_ebpf_object() -> Result<Ebpf> {
//     // 1) Prefer explicit path if provided
//     if let Ok(p) = env::var("CHAOSFILTER_EBPF_OBJ") {
//         let bytes = fs::read(&p)
//             .with_context(|| format!("failed to read ebpf object: {}", p))?;
//         return Ebpf::load(&bytes).context("failed to load eBPF object");
//     }

//     // 2) Fallback to OUT_DIR/chaosfilter-ebpf.o (will work later after build.rs/xtask is wired)
//     let out_dir = env::var("OUT_DIR")
//         .context("OUT_DIR not set; are you running via cargo?")?;

//     let mut path = PathBuf::from(out_dir);
//     path.push("chaosfilter-ebpf.o");

//     let bytes = fs::read(&path)
//         .with_context(|| format!("failed to read ebpf object: {}", path.display()))?;

//     Ebpf::load(&bytes).context("failed to load eBPF object")
// }