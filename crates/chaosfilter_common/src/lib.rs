use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub name: String,
    pub targets: Targets,
    pub schedule: Schedule,
    #[serde(default)]
    pub features: Features,
    #[serde(default)]
    pub injectors: Injectors,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Features {
    #[serde(default)]
    pub load_ebpf: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Injectors {
    #[serde(default)]
    pub qdisc_netem: QdiscNetem,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QdiscNetem {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub delay_ms: u32,

    #[serde(default)]
    pub loss_percent: f32,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Targets {
    #[serde(default)]
    pub cgroup: Option<String>,
    pub iface: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub duration_ms: u64,
}

impl Plan {
    pub fn load_from_toml_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let s = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        let plan: Plan = toml::from_str(&s)
            .with_context(|| format!("failed to parse TOML in: {}", path.display()))?;
        Ok(plan)
    }
}
