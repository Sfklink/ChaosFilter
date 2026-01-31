use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "chaosfilter",
    version,
    about = "Chaosfilter"
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Show current qdisc state for the interface
    Status { #[arg(long)] iface: String },

    /// Restore interface (delete root qdisc + clsact)
    Reset { #[arg(long)] iface: String },

    /// Run network chaos. Optionally scope to a cgroup (via tc classifier eBPF).
    Net {
        /// Target cgroup (e.g. my_cgroup or system.slice/nginx.service)
        #[arg(long)]
        cgroup: String,

        /// Network interface (e.g. eth0 or enp5s0)
        #[arg(long)]
        iface: Option<String>,

        /// Inject latency (e.g. 200ms)
        #[arg(long)]
        latency: Option<String>,

        /// Packet loss percentage (e.g. 5%)
        #[arg(long)]
        loss: Option<String>,

        /// How long to run the test (e.g. 30s)
        #[arg(long, default_value = "30s")]
        duration: String,
    }
}