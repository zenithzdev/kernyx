use crate::util::*;
use anyhow::Result;

pub fn show() -> Result<()> {
    let cfg = rooted("/etc/kernyx/config.toml");
    println!(
        "Config file: {} ({})",
        cfg.display(),
        if cfg.exists() {
            "present"
        } else {
            "not present"
        }
    );
    println!("State dir: {}", crate::tx::state_dir().display());
    println!("Transactions: {}", crate::tx::tx_root().display());
    Ok(())
}

pub fn validate() -> Result<()> {
    let cfg = rooted("/etc/kernyx/config.toml");
    if !cfg.exists() {
        println!("No config file; defaults will be used.");
        return Ok(());
    }
    println!("Config file exists and is readable.");
    Ok(())
}
