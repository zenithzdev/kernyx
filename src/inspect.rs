use crate::util::*;
use anyhow::Result;
use std::io::Write;

pub fn cpu() -> Result<()> {
    println!("CPU: {}", run_cmd_lossy("nproc", &[]));
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
    let out = run_cmd_lossy("df", &["-h"]);
    println!("{}", out);
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
    let fw = run_cmd_lossy("ufw", &["status"]);
    println!(
        "Firewall: {}",
        if fw.is_empty() {
            "ufw not available or inactive"
        } else {
            &fw
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
