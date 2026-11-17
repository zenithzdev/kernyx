use crate::hardware::Hardware;
use crate::util::*;
use anyhow::Result;

pub fn run(hw: &Hardware) -> Result<()> {
    println!("KERNYX Doctor");
    println!(
        "Root privileges: {}",
        if is_root() {
            "yes"
        } else {
            "no (apply will refuse)"
        }
    );
    println!(
        "Supported OS: {}",
        if hw.os_supported { "yes" } else { "no" }
    );
    println!("Virtualization: {}", hw.os_virt);
    println!("systemd running: {}", hw.os_systemd_running);
    println!(
        "BBR available: {}",
        hw.get("cap.has_bbr").cloned().unwrap_or_default()
    );
    println!(
        "FQ available: {}",
        hw.get("cap.has_fq").cloned().unwrap_or_default()
    );
    println!(
        "State dir writable: {}",
        if std::fs::create_dir_all(crate::tx::state_dir()).is_ok() {
            "yes"
        } else {
            "no"
        }
    );
    for t in [
        "lscpu",
        "lsblk",
        "ethtool",
        "sysctl",
        "systemctl",
        "udevadm",
    ] {
        println!(
            "  {:<12} {}",
            t,
            if have(t) { "available" } else { "missing" }
        );
    }
    Ok(())
}

fn is_root() -> bool {
    crate::util::run_cmd_lossy("id", &["-u"]) == "0"
}
