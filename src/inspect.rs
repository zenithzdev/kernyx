use crate::util::*;
use anyhow::Result;
use std::io::Write;

pub fn cpu() -> Result<()> {
    println!(
        "CPU: {}",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    );
    // model
    for line in std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
    {
        if let Some(v) = line.strip_prefix("model name") {
            if let Some(val) = v.split(':').nth(1) {
                println!("Model: {}", val.trim());
                break;
            }
        }
    }
    println!("Load: {}", read_file(std::path::Path::new("/proc/loadavg")));
    println!(
        "Governor: {}",
        read_file(&rooted(
            "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor"
        ))
    );
    Ok(())
}

pub fn memory() -> Result<()> {
    let m = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    for key in [
        "MemTotal:",
        "MemAvailable:",
        "MemFree:",
        "Cached:",
        "Buffers:",
        "SwapTotal:",
        "SwapFree:",
    ] {
        for line in m.lines() {
            if line.starts_with(key) {
                println!("{}", line.trim());
            }
        }
    }
    Ok(())
}

pub fn process() -> Result<()> {
    let out = run_cmd_lossy(
        "ps",
        &["-eo", "pid,pcpu,pmem,rss,stat,user,comm", "--sort=-pcpu"],
    );
    let lines: Vec<&str> = out.lines().take(26).collect();
    for l in lines {
        let _ = writeln!(std::io::stdout(), "{}", l);
    }
    Ok(())
}

pub fn network() -> Result<()> {
    println!(
        "{}",
        std::fs::read_to_string("/proc/net/dev").unwrap_or_default()
    );
    Ok(())
}

pub fn storage() -> Result<()> {
    println!("{:>12} {:>12} {:>12} Mountpoint", "Size", "Used", "Avail");
    for line in std::fs::read_to_string("/proc/mounts")
        .unwrap_or_default()
        .lines()
    {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 2 {
            continue;
        }
        let mount = cols[1];
        unsafe {
            let mut stat: libc::statvfs = std::mem::zeroed();
            let cpath = std::ffi::CString::new(mount).unwrap_or_default();
            if libc::statvfs(cpath.as_ptr(), &mut stat) == 0 {
                let size = stat.f_blocks as u64 * stat.f_frsize as u64 / 1_048_576;
                let avail = stat.f_bavail as u64 * stat.f_frsize as u64 / 1_048_576;
                let used = size.saturating_sub(avail);
                println!("{:>9} MiB {:>9} MiB {:>9} MiB {}", size, used, avail, mount);
            }
        }
    }
    Ok(())
}

pub fn services() -> Result<()> {
    let out = run_cmd_lossy(
        "systemctl",
        &[
            "list-units",
            "--type=service",
            "--state=failed",
            "--no-legend",
        ],
    );
    println!(
        "Failed services:\n{}",
        if out.is_empty() { "none" } else { &out }
    );
    Ok(())
}

pub fn security() -> Result<()> {
    println!(
        "SSH config size: {} bytes",
        std::fs::read_to_string(rooted("/etc/ssh/sshd_config"))
            .unwrap_or_default()
            .len()
    );
    let ufw_conf = std::fs::read_to_string(rooted("/etc/ufw/ufw.conf")).unwrap_or_default();
    let enabled = ufw_conf.lines().any(|l| l.trim() == "ENABLED=yes");
    println!(
        "Firewall: {}",
        if enabled {
            "ufw enabled"
        } else {
            "ufw not available or inactive"
        }
    );
    Ok(())
}

pub fn health() -> Result<()> {
    let m = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let total: u64 = m
        .lines()
        .find(|l| l.starts_with("MemTotal:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let avail: u64 = m
        .lines()
        .find(|l| l.starts_with("MemAvailable:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    println!("Memory available: {}%", avail * 100 / total);
    println!("Load: {}", read_file(std::path::Path::new("/proc/loadavg")));
    let failed = run_cmd_lossy(
        "systemctl",
        &[
            "list-units",
            "--type=service",
            "--state=failed",
            "--no-legend",
        ],
    );
    println!(
        "Failed services: {}",
        if failed.is_empty() { "none" } else { "present" }
    );
    Ok(())
}

pub fn benchmark() -> Result<()> {
    println!("Benchmarking is optional. Example: dd if=/dev/zero of=/tmp/kernyx-bench bs=1M count=256 oflag=direct conv=fsync");
    Ok(())
}
