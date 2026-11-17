use std::process::Command;

pub fn run() -> anyhow::Result<()> {
    let candidates = [
        "/usr/lib/kernyx/kernyx-monitor",
        "target/btop/kernyx-monitor",
        "/tmp/btop/bin/btop",
    ];
    for c in candidates {
        if std::path::Path::new(c).exists() {
            let status = Command::new(c)
                .arg("--config")
                .arg("/etc/kernyx/btop.conf")
                .status()?;
            if !status.success() {
                return Err(anyhow::anyhow!("monitor exited with status {}", status));
            }
            return Ok(());
        }
    }
    Err(anyhow::anyhow!(
        "Bundled monitor binary not found. Reinstall the kernyx package."
    ))
}
