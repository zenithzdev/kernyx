use std::process::Command;

pub fn run() -> anyhow::Result<()> {
    let candidates = [
        "/usr/lib/kernyx/kernyx-fetch",
        "target/fastfetch/build/fastfetch",
        "/tmp/fastfetch/build/fastfetch",
    ];
    for c in candidates {
        if std::path::Path::new(c).exists() {
            let status = Command::new(c).status()?;
            if !status.success() {
                return Err(anyhow::anyhow!("fetch exited with status {}", status));
            }
            return Ok(());
        }
    }
    Err(anyhow::anyhow!(
        "Bundled fetch binary not found. Reinstall the kernyx package."
    ))
}
