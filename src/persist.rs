use crate::plan::PlanItem;
use crate::tx::{is_dry_run, tx_backup_existing, tx_record, tx_update_state, TxItem};
use crate::util::*;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const SYSCTL_PATH: &str = "/etc/sysctl.d/99-kernyx-optimization.conf";
pub const LEGACY_PATH: &str = "/etc/sysctl.d/99-kernyx.conf";
pub const LEGACY_AHOPT_PATH: &str = "/etc/sysctl.d/99-ahopt.conf";
pub const TMPFILES_PATH: &str = "/etc/tmpfiles.d/kernyx.conf";
pub const UDEV_PATH: &str = "/etc/udev/rules.d/99-kernyx-io.rules";

#[derive(Debug, Clone)]
pub struct ConfigEntry {
    pub path: PathBuf,
    pub value: String,
}

pub fn parse_sysctl_file(path: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(content) = fs::read_to_string(path) else {
        return out;
    };
    for line in content.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('-') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !key.contains('.') {
            continue;
        }
        out.push((key.to_string(), value.trim().to_string()));
    }
    out
}

pub fn is_managed(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if name == "99-kernyx.conf" || name == "99-kernyx-optimization.conf" || name == "99-ahopt.conf"
    {
        return true;
    }
    let head = fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .take(8)
        .collect::<Vec<_>>()
        .join("\n");
    head.contains("Managed by kernyx") || head.contains("Managed by ahopt")
}

/// Returns explicit settings from unmanaged administrator configuration.
/// KERNYX never overrides a differing value owned by the administrator.
pub fn scan_sysctl_overrides() -> BTreeMap<String, ConfigEntry> {
    let mut files: Vec<PathBuf> = Vec::new();
    let dir = rooted("/etc/sysctl.d");
    if let Ok(rd) = fs::read_dir(&dir) {
        let mut names: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e == "conf").unwrap_or(false))
            .collect();
        names.sort();
        files.extend(names);
    }
    // /etc/sysctl.conf is applied after drop-ins by sysctl --system.
    let main = rooted("/etc/sysctl.conf");
    if main.exists() {
        files.push(main);
    }

    let mut map = BTreeMap::new();
    for file in files {
        if is_managed(&file) {
            continue;
        }
        for (key, value) in parse_sysctl_file(&file) {
            map.insert(
                key,
                ConfigEntry {
                    path: file.clone(),
                    value,
                },
            );
        }
    }
    map
}

pub fn legacy_managed() -> Vec<(PathBuf, BTreeMap<String, String>)> {
    let dir = rooted("/etc/sysctl.d");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().map(|ext| ext == "conf").unwrap_or(false))
        .filter(|path| path != &rooted(SYSCTL_PATH))
        .filter(|path| is_managed(path))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let values = parse_sysctl_file(&path).into_iter().collect();
            (path, values)
        })
        .collect()
}

fn final_sysctls(plan: &[PlanItem]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for item in plan {
        if item.kind != "sysctl" {
            continue;
        }
        match item.status.as_str() {
            "applied" | "verified" => {
                map.insert(item.target.clone(), item.value.clone());
            }
            // Keep values that were already at or above the recommendation.
            // Persist the live value so a reboot never lowers a higher ceiling.
            "unchanged" if !item.current.is_empty() => {
                map.insert(item.target.clone(), item.current.clone());
            }
            "skipped" if item.reason.starts_with("administrator override") => {
                map.remove(&item.target);
            }
            "unsupported" => {
                map.remove(&item.target);
            }
            _ => {}
        }
    }
    map
}

fn backup_required(path: &Path, id: &str) -> Result<Option<String>> {
    let backup = tx_backup_existing(path, id);
    if path.exists() && backup.as_deref().map(str::is_empty).unwrap_or(true) {
        return Err(anyhow::anyhow!(
            "could not create a rollback backup for {}; refusing to modify it",
            path.display()
        ));
    }
    Ok(backup)
}

fn managed_content(header: &str, lines: &[String]) -> String {
    let mut content = String::new();
    content.push_str(header);
    content.push_str("# Generated by KERNYX. Do not edit manually.\n");
    content.push_str("# Roll back with: sudo kernyx optimize rollback\n");
    for line in lines {
        content.push_str(line);
        content.push('\n');
    }
    content
}

fn write_file(path: &Path, content: &str, id: &str) -> Result<bool> {
    let current = fs::read_to_string(path).unwrap_or_default();
    if current == content {
        return Ok(false);
    }
    if is_dry_run() {
        println!("[dry-run] would write {}", path.display());
        return Ok(true);
    }
    let backup = backup_required(path, id)?;
    let old = sha256_file(path);
    tx_record(TxItem {
        id: id.to_string(),
        kind: "file".to_string(),
        target: path.to_string_lossy().to_string(),
        old,
        new: "written".to_string(),
        backup,
        extra: String::new(),
        state: "recorded".to_string(),
    });
    if let Err(e) = atomic_write(path, content, 0o644) {
        tx_update_state(id, "failed");
        return Err(e);
    }
    if fs::read_to_string(path).unwrap_or_default() != content {
        tx_update_state(id, "failed");
        return Err(anyhow::anyhow!(
            "verification failed after writing {}",
            path.display()
        ));
    }
    tx_update_state(id, "applied");
    Ok(true)
}

fn remove_file(path: &Path, id: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    if is_dry_run() {
        println!("[dry-run] would remove {}", path.display());
        return Ok(true);
    }
    let backup = backup_required(path, id)?;
    let old = sha256_file(path);
    tx_record(TxItem {
        id: id.to_string(),
        kind: "file".to_string(),
        target: path.to_string_lossy().to_string(),
        old,
        new: "missing".to_string(),
        backup,
        extra: String::new(),
        state: "recorded".to_string(),
    });
    fs::remove_file(path).with_context(|| format!("remove {}", path.display()))?;
    tx_update_state(id, "applied");
    Ok(true)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileChange {
    pub path: String,
    pub action: &'static str,
}

fn written_change(path: &Path) -> FileChange {
    FileChange {
        path: path.to_string_lossy().to_string(),
        action: "written",
    }
}

fn removed_change(path: &Path) -> FileChange {
    FileChange {
        path: path.to_string_lossy().to_string(),
        action: "removed",
    }
}

pub fn write_sysctl_dropin(plan: &[PlanItem]) -> Result<Vec<FileChange>> {
    let overrides = scan_sysctl_overrides();
    let mut desired: BTreeMap<String, String> = managed_keys().into_iter().collect();
    let legacy_files = legacy_managed();

    for (_, legacy) in &legacy_files {
        for (key, value) in legacy {
            if desired.contains_key(key) {
                continue;
            }
            if let Some(current) = overrides.get(key) {
                if !equivalent(&current.value, value) {
                    continue;
                }
            }
            desired.insert(key.clone(), value.clone());
        }
    }

    for (key, value) in final_sysctls(plan) {
        desired.insert(key, value);
    }

    // Dirty-page thresholds are mutually exclusive. Writing one form resets the
    // other, so a stale key in the managed drop-in would silently override the
    // profile on the next boot.
    let active = |target: &str| {
        plan.iter().any(|item| {
            item.kind == "sysctl"
                && item.target == target
                && matches!(item.status.as_str(), "applied" | "verified" | "unchanged")
        })
    };
    let uses_bytes = active("vm.dirty_bytes") || active("vm.dirty_background_bytes");
    let uses_ratio = active("vm.dirty_background_ratio") || active("vm.dirty_ratio");
    if uses_bytes && !uses_ratio {
        desired.remove("vm.dirty_ratio");
        desired.remove("vm.dirty_background_ratio");
    } else if uses_ratio && !uses_bytes {
        desired.remove("vm.dirty_bytes");
        desired.remove("vm.dirty_background_bytes");
    }

    // Administrator configuration always wins over a KERNYX default.
    desired.retain(|key, value| {
        overrides
            .get(key)
            .map(|entry| equivalent(&entry.value, value))
            .unwrap_or(true)
    });

    let mut written = Vec::new();
    let path = rooted(SYSCTL_PATH);
    if desired.is_empty() {
        if remove_file(&path, "persist.sysctl")? {
            written.push(removed_change(&path));
        }
    } else {
        let lines: Vec<String> = desired
            .iter()
            .map(|(key, value)| format!("{key} = {value}"))
            .collect();
        let content = managed_content("# Managed by kernyx (KERNYX optimizer)\n", &lines);
        if write_file(&path, &content, "persist.sysctl")? {
            written.push(written_change(&path));
        }
    }

    for (legacy, _) in &legacy_files {
        let name = legacy
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "legacy".to_string());
        let id = format!("persist.sysctl.legacy.{name}");
        if remove_file(legacy, &id)? {
            written.push(removed_change(legacy));
        }
    }
    Ok(written)
}

pub fn write_tmpfiles(plan: &[PlanItem]) -> Result<Vec<FileChange>> {
    let mut lines = Vec::new();
    for item in plan {
        if !matches!(item.status.as_str(), "applied" | "verified") {
            continue;
        }
        match item.kind.as_str() {
            "sysfs_choice" if item.target.contains("transparent_hugepage") => {
                lines.push(format!("w {} - - - - {}", item.target, item.value));
            }
            "cpu_governor" => {
                for path in cpu_paths("scaling_governor") {
                    lines.push(format!("w {path} - - - - {}", item.value));
                }
            }
            "cpu_epp" => {
                for path in cpu_paths("energy_performance_preference") {
                    lines.push(format!("w {path} - - - - {}", item.value));
                }
            }
            "rps" => {
                let mut paths = queue_paths(&item.target, "rx-", "rps_cpus");
                paths.extend(queue_paths(&item.target, "tx-", "xps_cpus"));
                for path in paths {
                    lines.push(format!("w {path} - - - - {}", item.value));
                }
            }
            _ => {}
        }
    }

    let path = rooted(TMPFILES_PATH);
    if lines.is_empty() {
        let mut written = Vec::new();
        if path.exists() && is_managed(&path) && remove_file(&path, "persist.tmpfiles")? {
            written.push(removed_change(&path));
        }
        return Ok(written);
    }
    lines.sort();
    lines.dedup();
    let content = managed_content("# Managed by kernyx (KERNYX boot persistence)\n", &lines);
    if write_file(&path, &content, "persist.tmpfiles")? {
        return Ok(vec![written_change(&path)]);
    }
    Ok(Vec::new())
}

fn cpu_paths(attr: &str) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..=255 {
        let path = format!("/sys/devices/system/cpu/cpu{i}/cpufreq/{attr}");
        if Path::new(&path).exists() {
            out.push(path);
        }
    }
    out
}

fn queue_paths(iface: &str, prefix: &str, attr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let base = rooted(&format!("/sys/class/net/{iface}/queues"));
    if let Ok(rd) = fs::read_dir(&base) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(prefix) {
                let path = entry.path().join(attr);
                if path.exists() {
                    out.push(path.to_string_lossy().to_string());
                }
            }
        }
    }
    out.sort();
    out
}

pub fn write_udev_rules(plan: &[PlanItem]) -> Result<Vec<FileChange>> {
    let mut lines = Vec::new();
    for item in plan {
        if !matches!(item.status.as_str(), "applied" | "verified") {
            continue;
        }
        match item.kind.as_str() {
            "io_scheduler" => lines.push(format!(
                "ACTION==\"add|change\", KERNEL==\"{}\", ATTR{{queue/scheduler}}=\"{}\"",
                item.target, item.value
            )),
            "sysfs" if item.target.ends_with("/queue/read_ahead_kb") => {
                if let Some(device) = device_from_queue_path(&item.target) {
                    lines.push(format!(
                        "ACTION==\"add|change\", KERNEL==\"{device}\", ATTR{{queue/read_ahead_kb}}=\"{}\"",
                        item.value
                    ));
                }
            }
            _ => {}
        }
    }

    let path = rooted(UDEV_PATH);
    if lines.is_empty() {
        let mut written = Vec::new();
        if path.exists() && is_managed(&path) && remove_file(&path, "persist.udev")? {
            written.push(removed_change(&path));
        }
        return Ok(written);
    }
    lines.sort();
    lines.dedup();
    let content = managed_content("# Managed by kernyx (KERNYX I/O persistence)\n", &lines);
    let mut written = Vec::new();
    if write_file(&path, &content, "persist.udev")? {
        written.push(written_change(&path));
    }
    Ok(written)
}

fn device_from_queue_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/sys/block/")?;
    let (device, _) = rest.split_once('/')?;
    Some(device.to_string())
}

pub fn apply_persistence(plan: &[PlanItem]) -> Result<Vec<FileChange>> {
    let mut written = Vec::new();
    written.extend(write_sysctl_dropin(plan)?);
    written.extend(write_tmpfiles(plan)?);
    written.extend(write_udev_rules(plan)?);
    Ok(written)
}

pub fn managed_keys() -> Vec<(String, String)> {
    let path = rooted(SYSCTL_PATH);
    if !path.exists() {
        return Vec::new();
    }
    parse_sysctl_file(&path)
}
