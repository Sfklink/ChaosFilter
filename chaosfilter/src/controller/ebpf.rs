use anyhow::{Context, Result};
use aya::programs::{tc, SchedClassifier, TcAttachType};
use aya::Ebpf;
#[rustfmt::skip]
use log::{debug, warn};

pub struct EbpfHandle {
    _ebpf: Ebpf,
}

pub async fn attach_classifier(iface: &str) -> Result<EbpfHandle> {
    let rlim = libc::rlimit {
        rlim_cur: libc::RLIM_INFINITY,
        rlim_max: libc::RLIM_INFINITY,
    };
    let ret = unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &rlim) };
    if ret != 0 {
        debug!("remove limit on locked memory failed, ret is: {ret}");
    }

    let mut ebpf = Ebpf::load(aya::include_bytes_aligned!(concat!(
        env!("OUT_DIR"),
        "/chaosfilter-ebpf"
    )))
    .context("failed to load embedded eBPF object")?;

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

    // harmless if already exists
    let _ = tc::qdisc_add_clsact(iface);

    let program: &mut SchedClassifier = ebpf
        .program_mut("chaosfilter")
        .context("failed to find eBPF program named `chaosfilter`")?
        .try_into()
        .context("failed to cast program to SchedClassifier")?;

    program.load().context("failed to load classifier")?;
    program
        .attach(iface, TcAttachType::Egress)
        .with_context(|| format!("failed to attach classifier to {iface}"))?;

    Ok(EbpfHandle { _ebpf: ebpf })
}
