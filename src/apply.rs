use crate::hardware::Hardware;
use crate::plan::PlanItem;
use crate::tx::*;
use crate::util::*;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn preflight(hw: &Hardware) -> Result<()> {
    if !is_dry_run() && !is_root() {
        return Err(anyhow::anyhow!(
            "Applying changes requires root. Re-run with sudo, or use --dry-run."
        ));
    }
    if hw.read_only_root {
        return Err(anyhow::anyhow!(
            "The root filesystem is read-only; KERNYX cannot persist changes."
        ));
    }
    if !is_dry_run() && !hw.proc_sys_writable && !hw.sysfs_writable {
        return Err(anyhow::anyhow!(
            "Neither /proc/sys nor /sys is writable in this environment."
        ));
    }
    Ok(())
}

fn is_root() -> bool {
    crate::util::is_root()
}

fn choice(raw: &str) -> String {
    if let Some(start) = raw.find('[') {
        if let Some(end) = raw[start + 1..].find(']') {
            return raw[start + 1..start + 1 + end].to_string();
        }
    }
    raw.split_whitespace().next().unwrap_or("").to_string()
}

fn record(id: &str, kind: &str, target: &str, old: &str, new: &str, extra: &str) {
    tx_record(TxItem {
        id: id.to_string(),
        kind: kind.to_string(),
        target: target.to_string(),
        old: old.to_string(),
        new: new.to_string(),
        backup: None,
        extra: extra.to_string(),
        state: "recorded".to_string(),
    });
}

fn fail(id: &str) {
    tx_update_state(id, "failed");
}

fn apply_sysctl(item: &PlanItem) -> Result<()> {
    let old = sysctl_read(&item.target);
    record(&item.id, "sysctl", &item.target, &old, &item.value, "");
    if is_dry_run() {
        return Ok(());
    }
    if let Err(e) = sysctl_write(&item.target, &item.value) {
        fail(&item.id);
        return Err(e);
    }
    let actual = sysctl_read(&item.target);
    if !writable_value_matches(&actual, &item.value) {
        if !old.is_empty() {
            let _ = sysctl_write(&item.target, &old);
        }
        fail(&item.id);
        return Err(anyhow::anyhow!(
            "{} did not accept {} (current: {})",
            item.target,
            item.value,
            actual
        ));
    }
    tx_update_state(&item.id, "applied");
    Ok(())
}

fn writable_value_matches(actual: &str, expected: &str) -> bool {
    if equivalent(actual, expected) {
        return true;
    }
    match (
        actual
            .split_whitespace()
            .last()
            .and_then(|v| v.parse::<i64>().ok()),
        expected
            .split_whitespace()
            .last()
            .and_then(|v| v.parse::<i64>().ok()),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn apply_sysfs(item: &PlanItem) -> Result<()> {
    let path = PathBuf::from(&item.target);
    if !path.exists() {
        fail(&item.id);
        return Err(anyhow::anyhow!("{} does not exist", item.target));
    }
    let old = read_file(&path);
    record(&item.id, "sysfs", &item.target, &old, &item.value, "");
    if is_dry_run() {
        return Ok(());
    }
    if let Err(e) = write_sysfs(&path, &item.value) {
        fail(&item.id);
        return Err(e);
    }
    let actual = read_file(&path);
    if !equivalent(&actual, &item.value) {
        let _ = write_sysfs(&path, &old);
        fail(&item.id);
        return Err(anyhow::anyhow!(
            "{} did not accept {} (current: {})",
            item.target,
            item.value,
            actual
        ));
    }
    tx_update_state(&item.id, "applied");
    Ok(())
}

fn apply_sysfs_choice(item: &PlanItem) -> Result<()> {
    let path = PathBuf::from(&item.target);
    if !path.exists() {
        fail(&item.id);
        return Err(anyhow::anyhow!("{} does not exist", item.target));
    }
    let old = choice(&read_file(&path));
    record(&item.id, "sysfs", &item.target, &old, &item.value, "");
    if is_dry_run() {
        return Ok(());
    }
    if let Err(e) = write_sysfs(&path, &item.value) {
        fail(&item.id);
        return Err(e);
    }
    if choice(&read_file(&path)) != item.value {
        let _ = write_sysfs(&path, &old);
        fail(&item.id);
        return Err(anyhow::anyhow!(
            "{} did not select {}",
            item.target,
            item.value
        ));
    }
    tx_update_state(&item.id, "applied");
    Ok(())
}

fn cpu_attr_paths(attr: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for i in 0..=255 {
        let raw = format!("/sys/devices/system/cpu/cpu{i}/cpufreq/{attr}");
        let path = rooted(&raw);
        if path.exists() {
            out.push(path);
        }
    }
    out
}

fn apply_cpu_attr(item: &PlanItem, attr: &str, kind: &str) -> Result<()> {
    let paths = cpu_attr_paths(attr);
    if paths.is_empty() {
        fail(&item.id);
        return Err(anyhow::anyhow!("no cpufreq policies found"));
    }
    let old: BTreeMap<String, String> = paths
        .iter()
        .map(|p| (p.to_string_lossy().to_string(), read_file(p)))
        .collect();
    let old_json = serde_json::to_string(&old).unwrap_or_default();
    record(&item.id, kind, "cpufreq", &old_json, &item.value, "");
    if is_dry_run() {
        return Ok(());
    }
    let mut written: Vec<PathBuf> = Vec::new();
    for path in &paths {
        if let Err(e) = write_sysfs(path, &item.value) {
            for done in &written {
                if let Some(previous) = old.get(&done.to_string_lossy().to_string()) {
                    let _ = write_sysfs(done, previous);
                }
            }
            fail(&item.id);
            return Err(e);
        }
        written.push(path.clone());
    }
    for path in &paths {
        if read_file(path) != item.value {
            for done in &written {
                if let Some(previous) = old.get(&done.to_string_lossy().to_string()) {
                    let _ = write_sysfs(done, previous);
                }
            }
            fail(&item.id);
            return Err(anyhow::anyhow!(
                "{} did not accept {} on {}",
                attr,
                item.value,
                path.display()
            ));
        }
    }
    tx_update_state(&item.id, "applied");
    Ok(())
}

fn scheduler_path(device: &str) -> PathBuf {
    rooted(&format!("/sys/block/{device}/queue/scheduler"))
}

fn apply_io_scheduler(item: &PlanItem) -> Result<()> {
    let path = scheduler_path(&item.target);
    if !path.exists() {
        fail(&item.id);
        return Err(anyhow::anyhow!(
            "scheduler path missing for {}",
            item.target
        ));
    }
    let old = choice(&read_file(&path));
    record(
        &item.id,
        "io_scheduler",
        &item.target,
        &old,
        &item.value,
        "",
    );
    if is_dry_run() {
        return Ok(());
    }
    if let Err(e) = write_sysfs(&path, &item.value) {
        fail(&item.id);
        return Err(e);
    }
    let actual = choice(&read_file(&path));
    if actual != item.value {
        let _ = write_sysfs(&path, &old);
        fail(&item.id);
        return Err(anyhow::anyhow!(
            "scheduler {} not accepted on {} (current: {})",
            item.value,
            item.target,
            actual
        ));
    }
    tx_update_state(&item.id, "applied");
    Ok(())
}

fn rps_paths(iface: &str, prefix: &str, attr: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let base = rooted(&format!("/sys/class/net/{iface}/queues"));
    if let Ok(rd) = fs::read_dir(&base) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(prefix) {
                let path = entry.path().join(attr);
                if path.exists() {
                    out.push(path);
                }
            }
        }
    }
    out.sort();
    out
}

fn apply_rps(item: &PlanItem) -> Result<()> {
    let mut paths = rps_paths(&item.target, "rx-", "rps_cpus");
    paths.extend(rps_paths(&item.target, "tx-", "xps_cpus"));
    if paths.is_empty() {
        fail(&item.id);
        return Err(anyhow::anyhow!(
            "no RPS/XPS queues found for {}",
            item.target
        ));
    }
    let old: BTreeMap<String, String> = paths
        .iter()
        .map(|p| (p.to_string_lossy().to_string(), read_file(p)))
        .collect();
    let old_json = serde_json::to_string(&old).unwrap_or_default();
    record(&item.id, "rps", &item.target, &old_json, &item.value, "");
    if is_dry_run() {
        return Ok(());
    }
    for path in &paths {
        if let Err(e) = write_sysfs(path, &item.value) {
            for done in &paths {
                if let Some(previous) = old.get(&done.to_string_lossy().to_string()) {
                    let _ = write_sysfs(done, previous);
                }
            }
            fail(&item.id);
            return Err(e).with_context(|| format!("write RPS {}", path.display()));
        }
    }
    for path in &paths {
        if read_file(path).trim_start_matches('0') != item.value.trim_start_matches('0') {
            for done in &paths {
                if let Some(previous) = old.get(&done.to_string_lossy().to_string()) {
                    let _ = write_sysfs(done, previous);
                }
            }
            fail(&item.id);
            return Err(anyhow::anyhow!(
                "RPS/XPS did not accept {} on {}",
                item.value,
                path.display()
            ));
        }
    }
    tx_update_state(&item.id, "applied");
    Ok(())
}

fn apply_swap(item: &PlanItem) -> Result<()> {
    let size = item
        .value
        .trim_end_matches("MiB")
        .trim()
        .parse::<u64>()
        .with_context(|| format!("invalid swap size: {}", item.value))?;
    crate::swap::create(size)?;
    tx_update_state(&item.id, "applied");
    Ok(())
}

pub fn apply_item(item: &PlanItem) -> Result<()> {
    match item.kind.as_str() {
        "sysctl" => apply_sysctl(item),
        "sysfs" => apply_sysfs(item),
        "sysfs_choice" => apply_sysfs_choice(item),
        "cpu_governor" => apply_cpu_attr(item, "scaling_governor", "cpu_governor"),
        "cpu_epp" => apply_cpu_attr(item, "energy_performance_preference", "cpu_epp"),
        "io_scheduler" => apply_io_scheduler(item),
        "rps" => apply_rps(item),
        "swap" => apply_swap(item),
        "advice" => Ok(()),
        other => Err(anyhow::anyhow!("unknown optimization kind: {other}")),
    }
}

fn verify_item(item: &PlanItem) -> Result<()> {
    match item.kind.as_str() {
        "sysctl" => {
            let actual = sysctl_read(&item.target);
            if writable_value_matches(&actual, &item.value) {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "{} is {} (expected {})",
                    item.target,
                    actual,
                    item.value
                ))
            }
        }
        "sysfs" => {
            let actual = read_file(Path::new(&item.target));
            if equivalent(&actual, &item.value) {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "{} is {} (expected {})",
                    item.target,
                    actual,
                    item.value
                ))
            }
        }
        "sysfs_choice" => {
            let actual = choice(&read_file(Path::new(&item.target)));
            if actual == item.value {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "{} selected {} (expected {})",
                    item.target,
                    actual,
                    item.value
                ))
            }
        }
        "cpu_governor" => verify_cpu_attr("scaling_governor", &item.value),
        "cpu_epp" => verify_cpu_attr("energy_performance_preference", &item.value),
        "io_scheduler" => {
            let actual = choice(&read_file(&scheduler_path(&item.target)));
            if actual == item.value {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "scheduler is {} (expected {})",
                    actual,
                    item.value
                ))
            }
        }
        "rps" => {
            let mut paths = rps_paths(&item.target, "rx-", "rps_cpus");
            paths.extend(rps_paths(&item.target, "tx-", "xps_cpus"));
            for path in paths {
                let actual = read_file(&path);
                if actual.trim_start_matches('0') != item.value.trim_start_matches('0') {
                    return Err(anyhow::anyhow!(
                        "RPS/XPS on {} is {} (expected {})",
                        path.display(),
                        actual,
                        item.value
                    ));
                }
            }
            Ok(())
        }
        "swap" => {
            if crate::swap::verify_active(Path::new(&item.target)) {
                Ok(())
            } else {
                Err(anyhow::anyhow!("swap {} is not active", item.target))
            }
        }
        "advice" => Ok(()),
        _ => Ok(()),
    }
}

fn verify_cpu_attr(attr: &str, expected: &str) -> Result<()> {
    for path in cpu_attr_paths(attr) {
        let actual = read_file(&path);
        if actual != expected {
            return Err(anyhow::anyhow!(
                "{} is {} (expected {})",
                path.display(),
                actual,
                expected
            ));
        }
    }
    Ok(())
}

pub fn verify_plan(plan: &mut [PlanItem]) -> (usize, usize) {
    let mut verified = 0;
    let mut failed = 0;
    for item in plan.iter_mut() {
        if item.status != "applied" {
            continue;
        }
        match verify_item(item) {
            Ok(()) => {
                item.status = "verified".to_string();
                verified += 1;
            }
            Err(e) => {
                item.status = "failed".to_string();
                item.detail = e.to_string();
                failed += 1;
            }
        }
    }
    (verified, failed)
}

fn restore_file(item: &TxItem) -> Result<()> {
    let target = rooted(&item.target);
    if item.old == "missing" {
        if target.exists() {
            fs::remove_file(&target)?;
        }
        return Ok(());
    }
    if let Some(backup) = &item.backup {
        if !backup.is_empty() {
            rooted_backup_restore(backup, &target)?;
            return Ok(());
        }
    }
    Err(anyhow::anyhow!("no backup available for {}", item.target))
}

fn restore_map(value: &str, kind: &str) -> Result<()> {
    let map: BTreeMap<String, String> = serde_json::from_str(value).unwrap_or_default();
    let mut errors = Vec::new();
    for (path, old) in map {
        if let Err(e) = write_sysfs(Path::new(&path), &old) {
            errors.push(format!("{kind} {path}: {e}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(errors.join("; ")))
    }
}

pub fn transaction_rollback(tx_id: &str, reason: &str) -> Result<()> {
    let dir = tx_root().join(tx_id);
    if !dir.exists() {
        return Err(anyhow::anyhow!("transaction {} does not exist", tx_id));
    }
    std::env::set_var("KERNYX_TX_DIR", dir.to_string_lossy().to_string());
    std::env::set_var("KERNYX_TX_ID", tx_id);
    let items = load_manifest();
    let mut failures = Vec::new();
    let mut restored = 0;

    for item in items.iter().rev() {
        if !matches!(item.state.as_str(), "applied" | "recorded" | "failed") {
            continue;
        }
        let result = match item.kind.as_str() {
            "sysctl" => {
                if item.old.is_empty() {
                    Ok(())
                } else {
                    sysctl_write(&item.target, &item.old)
                }
            }
            "sysfs" => {
                if item.old.is_empty() {
                    Ok(())
                } else {
                    write_sysfs(Path::new(&item.target), &item.old)
                }
            }
            "cpu_governor" => restore_map(&item.old, "governor"),
            "cpu_epp" => restore_map(&item.old, "EPP"),
            "rps" => restore_map(&item.old, "RPS/XPS"),
            "io_scheduler" => {
                if item.old.is_empty() {
                    Ok(())
                } else {
                    write_sysfs(&scheduler_path(&item.target), &item.old)
                }
            }
            "swap" => crate::swap::remove(Path::new(&item.target)),
            "file" => restore_file(item),
            _ => Ok(()),
        };
        match result {
            Ok(()) => {
                restored += 1;
                tx_update_state(&item.id, "rolled_back");
            }
            Err(e) => failures.push(format!("{}: {e}", item.id)),
        }
    }

    tx_set_status(
        "rolled_back",
        Some(if failures.is_empty() {
            reason
        } else {
            "rollback completed with errors"
        }),
    );
    tx_end_cleanup_dryrun();

    if failures.is_empty() {
        println!("Rolled back transaction {} ({} items).", tx_id, restored);
        Ok(())
    } else {
        for failure in &failures {
            eprintln!("rollback warning: {failure}");
        }
        Err(anyhow::anyhow!(
            "rollback completed with {} error(s)",
            failures.len()
        ))
    }
}
