use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "chaosfilter")]
#[command(about = "Chaos testing CLI for Linux networking")]
pub struct Cli {
    /// Target cgroup (e.g. my_cgroup or system.slice/nginx.service)
    #[arg(long)]
    pub cgroup: String,

    /// Network interface (e.g. eth0)
    #[arg(long)]
    pub iface: Option<String>,

    /// Inject latency (e.g. 200ms)
    #[arg(long)]
    pub latency: Option<String>,

    /// Packet loss percentage (e.g. 5%)
    #[arg(long)]
    pub loss: Option<String>,

    /// How long to run the test (e.g. 30s)
    #[arg(long, default_value = "30s")]
    pub duration: String,
}
