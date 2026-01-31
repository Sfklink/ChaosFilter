use anyhow::Result;
use crate::ping::ping_path;

pub fn compare_with_control(modified_iface: &str) -> Result<()> {
    println!("\n=== Network Comparison (10 pings) ===\n");

    // CONTROL path (never modified)
    let control = ping_path("control", "10.0.1.1", 10)?;

    // MODIFIED path
    let modified = match modified_iface {
        "vethA" => ping_path("vethA", "10.0.0.1", 10)?,
        _ => {
            println!("No comparison path defined for {modified_iface}");
            return Ok(());
        }
    };

    println!("CONTROL (baseline):");
    println!("  transmitted : {}", control.transmitted);
    println!("  received    : {}", control.received);
    println!("  loss %      : {}", control.loss_pct);
    println!(
        "  rtt (ms)    : min {:.2} | avg {:.2} | max {:.2}",
        control.min_ms, control.avg_ms, control.max_ms
    );

    println!("\nMODIFIED ({modified_iface}): {modified}");
    println!("====================================");

    Ok(())
}
