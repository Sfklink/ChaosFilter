use anyhow::{Context, Result};
use aya::{
    maps::HashMap,
    programs::{tc,
               SchedClassifier, TcAttachType},
    Ebpf,
};
use aya_log::EbpfLogger;
#[rustfmt::skip]
use log::{debug, warn};

pub struct EbpfHandle {
    _ebpf: Ebpf,
}

pub fn attach_classifier(iface: &str, cgroups: &[u64]) -> Result<EbpfHandle> {
    let mut ebpf = Ebpf::load(aya::include_bytes_aligned!(concat!(
        env!("OUT_DIR"),
        "/chaosfilter-ebpf"
    )))
        .context("failed to load embedded eBPF object")?;

    let _logger = match EbpfLogger::init(&mut ebpf) {
        Ok(logger) => {
            debug!("[ebpf] logger initialized");
            Some(logger)
        }
        Err(e) => {
            warn!("failed to initialize eBPF logger: {e}");
            None
        }
    };

    {
        let map = ebpf
            .map_mut("TARGET_CGROUPS")
            .context("TARGET_CGROUPS map not found")?;

        let mut targets: HashMap<_, u64, u8> =
            HashMap::try_from(map).context("failed to open TARGET_CGROUPS")?;

        for id in cgroups {
            targets.insert(*id, 1, 0)?;
        }
    }

    let program: &mut SchedClassifier = ebpf
        .program_mut("chaosfilter")
        .context("failed to find eBPF program named `chaosfilter`")?
        .try_into()
        .context("failed to cast program to SchedClassifier")?;

    program.load().context("failed to load classifier")?;
    program.attach(iface, TcAttachType::Egress)?;

    Ok(EbpfHandle {
        _ebpf: ebpf,
    })
}