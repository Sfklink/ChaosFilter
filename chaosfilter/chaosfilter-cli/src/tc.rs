use anyhow::{anyhow, Context, Result};
use futures_util::TryStreamExt;
use netlink_packet_route::tc::TcHandle;
use rtnetlink::new_connection;

/// Show qdisc state for a single interface.
///
/// This is intended to be the programmatic equivalent of:
///   tc qdisc show dev <iface>
pub fn status(iface: &str) -> Result<String> {
    let rt = tokio::runtime::Runtime::new().context("create tokio runtime")?;
    rt.block_on(async move { status_async(iface).await })
}

/// Reset qdisc state for a single interface (best effort).
///
/// Equivalent intent to:
///   sudo tc qdisc del dev <iface> root
///
/// Note: On many systems, deleting the root qdisc triggers the kernel to recreate
/// a default qdisc (often fq_codel). This is considered "normal" but not guaranteed
/// to match the prior exact configuration unless you capture and restore it.
pub fn reset(iface: &str) -> Result<()> {
    let rt = tokio::runtime::Runtime::new().context("create tokio runtime")?;
    rt.block_on(async move { reset_async(iface).await })
}

async fn status_async(iface: &str) -> Result<String> {
    let (connection, handle, _) = new_connection().context("new_connection() failed")?;
    tokio::spawn(connection);

    let ifindex = iface_index(&handle, iface).await?;

    let mut out = String::new();
    out.push_str(&format!("Interface {iface} (ifindex={ifindex}) qdiscs:\n"));

    let mut q = handle.qdisc().get().index(ifindex).execute();
    while let Some(msg) = q.try_next().await? {
        // Defensive filter: some kernels/drivers can return extra qdiscs; we only want this iface.
        if msg.header.index != ifindex as u32 {
            continue;
        }

        let mut kind: Option<String> = None;
        for a in &msg.attributes {
            if let netlink_packet_route::tc::TcAttribute::Kind(k) = a {
                kind = Some(k.clone());
            }
        }

        out.push_str(&format!(
            "- kind={:<10} handle={} parent={}\n",
            kind.unwrap_or_else(|| "<unknown>".into()),
            fmt_handle(msg.header.handle),
            fmt_handle(msg.header.parent),
        ));
    }

    Ok(out)
}

async fn reset_async(iface: &str) -> Result<()> {
    let (connection, handle, _) = new_connection().context("new_connection() failed")?;
    tokio::spawn(connection);

    let ifindex = iface_index(&handle, iface).await?;

    // Delete root qdisc. If it doesn't exist (or was already deleted), return a clean error.
    let mut req = handle.qdisc().del(ifindex);
    let msg = req.message_mut();
    msg.header.parent = TcHandle::ROOT;
    msg.header.handle = TcHandle::from(0);

    req.execute()
        .await
        .map_err(|e| anyhow!("delete root qdisc failed on {iface}: {e}"))?;

    Ok(())
}

async fn iface_index(handle: &rtnetlink::Handle, name: &str) -> Result<i32> {
    let mut links = handle.link().get().match_name(name.to_string()).execute();
    let link = links
        .try_next()
        .await?
        .ok_or_else(|| anyhow!("No such interface: {name}"))?;
    Ok(link.header.index as i32)
}

fn fmt_handle(h: TcHandle) -> String {
    // Render as major:minor (like tc does) when possible.
    if h == TcHandle::ROOT {
        return "root".to_string();
    }
    format!("{}:{}", h.major, h.minor)
}