use std::process::Command;

pub fn run(command: &[String]) -> anyhow::Result<()> {
    if command.is_empty() {
        println!("Usage: kernyx benchmark -- <command...>");
        return Ok(());
    }
    let candidates = [
        "/usr/lib/kernyx/kernyx-bench",
        "target/hyperfine/target/release/hyperfine",
        "/tmp/hyperfine/target/release/hyperfine",
    ];
    for c in candidates {
        if std::path::Path::new(c).exists() {
            let status = Command::new(c).args(command).status()?;
            if !status.success() {
                return Err(anyhow::anyhow!("benchmark exited with status {}", status));
            }
            return Ok(());
        }
    }
    Err(anyhow::anyhow!(
        "Bundled benchmark binary not found. Reinstall the kernyx package."
    ))
}
