use crate::hardware::Hardware;
use crate::tx::{is_dry_run, tx_backup_existing, tx_record, tx_update_state, TxItem};
use crate::util::*;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct SwapEntry {
    pub filename: String,
    pub kind: String,
    pub size_kb: u64,
    pub used_kb: u64,
}

pub fn active_entries() -> Vec<SwapEntry> {
    let mut out = Vec::new();
    for line in read_file(&rooted("/proc/swaps")).lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 5 {
            out.push(SwapEntry {
                filename: cols[0].to_string(),
                kind: cols[1].to_string(),
                size_kb: cols[2].parse().unwrap_or(0),
                used_kb: cols[3].parse().unwrap_or(0),
            });
        }
    }
    out
}

pub fn recommended_mb(hw: &Hardware) -> u64 {
    match hw.ram_mb {
        0..=1024 => 2048,
        1025..=4096 => 4096,
        4097..=16384 => 4096,
        16385..=32768 => 8192,
        _ => 8192,
    }
}

pub fn parse_size(input: &str) -> Result<u64> {
    let raw = input.trim().to_lowercase();
    if raw.is_empty() {
        return Err(anyhow::anyhow!("empty swap size"));
    }
    let (number, multiplier) = if let Some(v) = raw.strip_suffix("gib") {
        (v, 1024)
    } else if let Some(v) = raw.strip_suffix("gb") {
        (v, 1024)
    } else if let Some(v) = raw.strip_suffix('g') {
        (v, 1024)
    } else if let Some(v) = raw.strip_suffix("mib") {
        (v, 1)
    } else if let Some(v) = raw.strip_suffix("mb") {
        (v, 1)
    } else if let Some(v) = raw.strip_suffix('m') {
        (v, 1)
    } else {
        (raw.as_str(), 1)
    };
    let number: f64 = number
        .trim()
        .parse()
        .with_context(|| format!("invalid swap size: {input}"))?;
    let mb = (number * multiplier as f64).round() as u64;
    if !(128..=65_536).contains(&mb) {
        return Err(anyhow::anyhow!(
            "swap size must be between 128 MiB and 64 GiB"
        ));
    }
    Ok(mb)
}

pub fn default_path() -> PathBuf {
    for candidate in ["/swapfile", "/var/lib/kernyx/swapfile", "/swapfile.kernyx"] {
        let path = rooted(candidate);
        if !path.exists() {
            return path;
        }
    }
    rooted("/var/lib/kernyx/swapfile.kernyx")
}

pub fn available_space_mb() -> Option<u64> {
    let out = run_cmd_lossy("df", &["-B1", "--output=avail", "/"]);
    let value = out
        .lines()
        .nth(1)
        .and_then(|l| l.trim().parse::<u64>().ok());
    if value.is_some() {
        return value.map(|b| b / 1_048_576);
    }
    let fallback = run_cmd_lossy("df", &["-Pk", "/"]);
    fallback
        .lines()
        .nth(1)
        .and_then(|l| l.split_whitespace().nth(3))
        .and_then(|v| v.parse::<u64>().ok())
        .map(|kb| kb / 1024)
}

pub fn fstype() -> String {
    let out = run_cmd_lossy("findmnt", &["-no", "FSTYPE", "/"]);
    if !out.is_empty() {
        return out;
    }
    for line in read_file(&rooted("/proc/mounts")).lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 3 && cols[1] == "/" {
            return cols[2].to_string();
        }
    }
    String::new()
}

fn append_fstab(path: &Path) -> Result<()> {
    let fstab_path = rooted("/etc/fstab");
    if !fstab_path.exists() {
        return Err(anyhow::anyhow!(
            "/etc/fstab does not exist; refusing to create it automatically"
        ));
    }
    let existing = fs::read_to_string(&fstab_path).unwrap_or_default();
    if existing.lines().any(|line| {
        !line.trim_start().starts_with('#') && line.contains(&path.to_string_lossy().to_string())
    }) {
        return Ok(());
    }
    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str("# kernyx managed swap\n");
    content.push_str(&format!("{} none swap sw 0 0\n", path.display()));

    let backup = tx_backup_existing(&fstab_path, "swap.fstab");
    if backup.as_deref().map(str::is_empty).unwrap_or(true) {
        return Err(anyhow::anyhow!(
            "could not back up /etc/fstab; refusing to modify it"
        ));
    }
    let old = sha256_file(&fstab_path);
    tx_record(TxItem {
        id: "swap.fstab".to_string(),
        kind: "file".to_string(),
        target: fstab_path.to_string_lossy().to_string(),
        old,
        new: "with kernyx swap entry".to_string(),
        backup,
        extra: String::new(),
        state: "recorded".to_string(),
    });
    atomic_write(&fstab_path, &content, 0o644)?;
    tx_update_state("swap.fstab", "applied");
    Ok(())
}

fn create_file(path: &Path, size_mb: u64) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let fs_type = fstype();
    let path_str = path.to_string_lossy();
    if fs_type == "btrfs" {
        let size = format!("{size_mb}m");
        let status = std::process::Command::new("btrfs")
            .args([
                "filesystem",
                "mkswapfile",
                "--size",
                &size,
                path_str.as_ref(),
            ])
            .status();
        if let Ok(s) = status {
            if s.success() && path.exists() {
                return Ok(());
            }
        }
    }

    let size_arg = format!("{size_mb}M");
    let fallocate = std::process::Command::new("fallocate")
        .args(["-l", &size_arg, path_str.as_ref()])
        .status();
    if fallocate.map(|s| s.success()).unwrap_or(false) && path.exists() {
        return Ok(());
    }

    let status = std::process::Command::new("dd")
        .args([
            "if=/dev/zero",
            &format!("of={}", path.display()),
            "bs=1M",
            &format!("count={size_mb}"),
            "status=none",
        ])
        .status()
        .context("dd is required to create a swap file")?;
    if !status.success() {
        return Err(anyhow::anyhow!("failed to allocate swap file"));
    }
    Ok(())
}

fn enable_swap(path: &Path) -> Result<()> {
    let output = std::process::Command::new("mkswap")
        .arg(path)
        .output()
        .context("mkswap failed")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "mkswap failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let output = std::process::Command::new("swapon")
        .arg(path)
        .output()
        .context("swapon failed")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "swapon failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

pub fn verify_active(path: &Path) -> bool {
    let needle = path.to_string_lossy();
    active_entries()
        .iter()
        .any(|entry| entry.filename == needle || entry.filename.ends_with(needle.as_ref()))
}

pub fn create(size_mb: u64) -> Result<PathBuf> {
    if is_dry_run() {
        return Ok(default_path());
    }
    if run_cmd_lossy("id", &["-u"]) != "0" {
        return Err(anyhow::anyhow!("creating swap requires root"));
    }
    if fstype().is_empty() {
        return Err(anyhow::anyhow!("cannot determine root filesystem"));
    }
    let path = default_path();
    if path.exists() {
        return Err(anyhow::anyhow!(
            "swap target already exists: {}",
            path.display()
        ));
    }
    if let Some(avail) = available_space_mb() {
        let required = size_mb.saturating_add(size_mb / 10).saturating_add(64);
        if avail < required {
            return Err(anyhow::anyhow!(
                "not enough free space: need {} MiB, have {} MiB",
                required,
                avail
            ));
        }
    }

    create_file(&path, size_mb)?;
    let path_str = path.to_string_lossy();
    let chmod = std::process::Command::new("chmod")
        .args(["600", path_str.as_ref()])
        .status();
    if !chmod.map(|s| s.success()).unwrap_or(false) {
        let _ = fs::remove_file(&path);
        return Err(anyhow::anyhow!("failed to set swap file permissions"));
    }
    if let Err(e) = enable_swap(&path) {
        let _ = fs::remove_file(&path);
        return Err(e);
    }
    if let Err(e) = append_fstab(&path) {
        let _ = std::process::Command::new("swapoff").arg(&path).status();
        let _ = fs::remove_file(&path);
        return Err(e);
    }

    let backup = None;
    tx_record(TxItem {
        id: "swap.create".to_string(),
        kind: "swap".to_string(),
        target: path.to_string_lossy().to_string(),
        old: "none".to_string(),
        new: format!("{size_mb}MiB"),
        backup,
        extra: "fstab=kernyx managed swap".to_string(),
        state: "recorded".to_string(),
    });
    if !verify_active(&path) {
        let _ = std::process::Command::new("swapoff").arg(&path).status();
        let _ = fs::remove_file(&path);
        tx_update_state("swap.create", "failed");
        return Err(anyhow::anyhow!("swap did not become active"));
    }
    tx_update_state("swap.create", "applied");
    Ok(path)
}

pub fn remove(path: &Path) -> Result<()> {
    let target = path.to_string_lossy();
    let active = active_entries()
        .iter()
        .any(|entry| entry.filename == target || entry.filename.ends_with(target.as_ref()));
    if active {
        let output = std::process::Command::new("swapoff")
            .arg(path)
            .output()
            .with_context(|| format!("swapoff {}", path.display()))?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "swapoff {} failed: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

pub fn summary(hw: &Hardware) -> Vec<String> {
    let mut lines = Vec::new();
    let entries = active_entries();
    if entries.is_empty() {
        lines.push(format!("none (recommended {} MiB)", recommended_mb(hw)));
    } else {
        for entry in entries {
            lines.push(format!(
                "{} ({}) {} MiB used {} MiB",
                entry.filename,
                entry.kind,
                entry.size_kb / 1024,
                entry.used_kb / 1024
            ));
        }
    }
    if hw.zram_active {
        lines.push("zram active".to_string());
    }
    if hw.zswap_enabled {
        lines.push("zswap enabled".to_string());
    }
    lines
}
