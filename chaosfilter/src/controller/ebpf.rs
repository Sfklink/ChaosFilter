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



/*
  So this doesn't belong here, it was in main.rs, but I need to go to bed.  .
  I am committing just comments.

  This belongs in qdiscs.rs, or at least to as something called there.
  Easiest way to check this is something
    if plan.injectors.network_config.network_ebpf_cgroup:
        attach_classifier(plan.injectors.network_config.iface,
                         plan.injectors.network_config.network_ebpf_cgroup)

 */
// if let Some(cgroup) = plan.targets.cgroup.as_deref() {
//     let target_cgroup_id: u64 = cgroup
//         .parse()
//         .context("targets.cgroup must be a numeric cgroup id")?;
//     let _ebpf = attach_classifier(&iface, &[target_cgroup_id]).await?;

/*

Writing these so  I get a better idea of how this is supposed to go.
Set an rlimit
 */
pub async fn attach_classifier(iface: &str, target_cgroup_ids: &[u64]) -> Result<EbpfHandle> {
    /*
    https://www.man7.org/linux/man-pages/man2/getrlimit.2.html
    Why don't we just functionalize this section for ebpf-cgroup manipulation
     */
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

    {
        let map = ebpf
            .map_mut("TARGET_CGROUPS")
            .context("TARGET_CGROUPS map not found")?;

        let mut targets: HashMap<_, u64, u8> =
            HashMap::try_from(map).context("failed to open TARGET_CGROUPS")?;

        for id in target_cgroup_ids {
            targets
                .insert(*id, 1, 0)
                .with_context(|| format!("failed to insert target cgroup id {id}"))?;
        }
    }
    let _ = tc::qdisc_add_clsact(iface);

    /*
    ah gotcha so here's where we're actually calling the thing in chaosfilter-ebpf
     */
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
