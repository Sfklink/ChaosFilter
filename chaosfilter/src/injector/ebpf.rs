use anyhow::{Context, Result};
use aya::{
    maps::HashMap,
    programs::{tc, SchedClassifier, TcAttachType},
    Ebpf,
};
#[rustfmt::skip]
use log::{debug, warn};

pub struct EbpfHandle {
    _ebpf: Ebpf,
}


pub fn attach_classifier(iface: &str, cgroups: &[u64]) -> Result<EbpfHandle> {
    /*
    https://www.man7.org/linux/man-pages/man2/getrlimit.2.html
    Why don't we just functionalize this section for ebpf-cgroup manipulation
     */
    println!("[ebpf] Setting rlimit");
    let rlim = libc::rlimit {
        rlim_cur: libc::RLIM_INFINITY,
        rlim_max: libc::RLIM_INFINITY,
    };

    let ret = unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &rlim) };
    if ret != 0 {
        debug!("remove limit on locked memory failed, ret is: {ret}");
    }
    println!("[ebpf] rlimit set successful");

    let mut ebpf = Ebpf::load(aya::include_bytes_aligned!(concat!(
        env!("OUT_DIR"),
        "/chaosfilter-ebpf"
    )))
    .context("failed to load embedded eBPF object")?;
    println!("[ebpf] Loaded eBPF object chaosfilter-ebpf");


    match aya_log::EbpfLogger::init(&mut ebpf) {
        Err(e) => {
            warn!("failed to initialize eBPF logger: {e}");
        }
        Ok(logger) => {
            let mut logger =
                tokio::io::unix::AsyncFd::with_interest(logger, tokio::io::Interest::READABLE)?;
            tokio::task::spawn(async move {
                loop {
                    let mut guard = logger.readable_mut().await.unwrap();
                    guard.get_inner_mut().flush();
                    guard.clear_ready();
                }
            });
        }
    }
    println!("[ebpf] logger initialized");
    {
        let map = ebpf
            .map_mut("TARGET_CGROUPS")
            .context("TARGET_CGROUPS map not found")?;

        println!("[ebpf] opening TARGET_CGROUPS");
        let mut targets: HashMap<_, u64, u8> =
            HashMap::try_from(map).context("failed to open TARGET_CGROUPS")?;

        println!("[ebpf] inserting target cgroups");
        for id in cgroups {
            targets
                .insert(*id, 1, 0)
                .with_context(|| format!("failed to insert target cgroup id {id}"))?;
        }
    }
    println!("[ebpf] adding clsact qdisc");
    let _ = tc::qdisc_add_clsact(iface);

    /*
    ah gotcha so here's where we're actually calling the thing in chaosfilter-ebpf
     */
    println!("[ebpf] attach_classifier, attempting to attach ebpf program.");

    let program: &mut SchedClassifier = ebpf
        .program_mut("chaosfilter")
        .context("failed to find eBPF program named `chaosfilter`")?
        .try_into()
        .context("failed to cast program to SchedClassifier")?;

    program.load().context("failed to load classifier")?;
    program
        .attach(iface, TcAttachType::Egress)
        .with_context(|| format!("failed to attach classifier to {iface}"))?;
    println!("[ebpf] classifier attached, handle returned.");
    Ok(EbpfHandle { _ebpf: ebpf })
}
