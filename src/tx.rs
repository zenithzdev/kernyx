use crate::hardware::Hardware;
use crate::plan::PlanItem;
use crate::util::*;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxItem {
    pub id: String,
    pub kind: String,
    pub target: String,
    pub old: String,
    pub new: String,
    pub backup: Option<String>,
    pub extra: String,
    pub state: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxMeta {
    pub txid: String,
    pub created: String,
    pub profile: String,
    pub profile_name: String,
    pub hostname: String,
    pub kernel: String,
    pub cpu: String,
    pub cpu_effective: i64,
    pub ram_mb: i64,
    pub virtualized: bool,
    pub pve: bool,
    pub cloud: String,
    pub version: String,
    pub status: String,
    pub reason: Option<String>,
}

pub fn state_dir() -> PathBuf {
    std::env::var("KERNYX_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/var/lib/kernyx"))
}

pub fn tx_root() -> PathBuf {
    state_dir().join("transactions")
}

pub fn tx_dir(id: &str) -> PathBuf {
    tx_root().join(id)
}

pub fn is_dry_run() -> bool {
    std::env::var("KERNYX_DRY_RUN")
        .map(|v| v == "1")
        .unwrap_or(false)
}

pub fn tx_begin(profile: &str, hw: &Hardware, plan: &[PlanItem]) -> Result<String> {
    let id = if is_dry_run() {
        format!("dryrun-{}-{}", std::process::id(), rand_suffix())
    } else {
        format!("{}-{}", timestamp_id(), rand_suffix())
    };
    let dir = tx_dir(&id);
    if is_dry_run() {
        // scratch under TMPDIR
        let scratch = std::env::temp_dir().join(format!("kernyx-dryrun.{}", rand_suffix()));
        fs::create_dir_all(&scratch).context("scratch")?;
        // use scratch as TX dir by env? We'll store path in a global not possible; return id and caller uses tx_path()
        // Simpler: store real tx under state? Requirement dry-run no state. We'll use scratch dir and record in env var KERNYX_TX_DIR
        std::env::set_var("KERNYX_TX_DIR", scratch.to_string_lossy().to_string());
        std::env::set_var("KERNYX_TX_ID", id.clone());
        fs::create_dir_all(scratch.join("files"))?;
        fs::create_dir_all(scratch.join("details"))?;
        // write meta/plan into scratch
        write_meta(&scratch, &id, profile, hw)?;
        serde_json::to_string_pretty(plan)
            .ok()
            .map(|s| fs::write(scratch.join("plan.json"), s).ok());
        serde_json::to_string_pretty(&Vec::<TxItem>::new())
            .ok()
            .map(|s| fs::write(scratch.join("manifest.json"), s).ok());
        return Ok(id);
    }
    ensure_dir(&tx_root())?;
    ensure_dir(&dir)?;
    ensure_dir(&dir.join("files"))?;
    ensure_dir(&dir.join("details"))?;
    std::env::set_var("KERNYX_TX_DIR", dir.to_string_lossy().to_string());
    std::env::set_var("KERNYX_TX_ID", id.clone());
    write_meta(&dir, &id, profile, hw)?;
    serde_json::to_string_pretty(plan)
        .ok()
        .map(|s| fs::write(dir.join("plan.json"), s).ok());
    serde_json::to_string_pretty(&Vec::<TxItem>::new())
        .ok()
        .map(|s| fs::write(dir.join("manifest.json"), s).ok());
    Ok(id)
}

fn write_meta(dir: &Path, id: &str, profile: &str, hw: &Hardware) -> Result<()> {
    let content = format!(
        "txid={}\ncreated={}\nprofile={}\nprofile_name={}\nhostname={}\nkernel={}\ncpu={}\ncpu_effective={}\nram_mb={}\nvirtual={}\npve={}\ncloud={}\nversion=2.0.0\nstatus=open\n",
        id, now_iso(), profile, profile, run_cmd_lossy("hostname", &[]), run_cmd_lossy("uname", &["-r"]),
        hw.cpu_model, hw.cpu_effective, hw.ram_mb, hw.is_virtual, hw.os_pve, hw.os_cloud
    );
    fs::write(dir.join("meta"), content)?;
    Ok(())
}

pub fn current_tx_dir() -> PathBuf {
    std::env::var("KERNYX_TX_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(""))
}

#[allow(dead_code)]
pub fn current_tx_id() -> String {
    std::env::var("KERNYX_TX_ID").unwrap_or_default()
}

pub fn tx_set_status(status: &str, reason: Option<&str>) {
    let dir = current_tx_dir();
    let meta = dir.join("meta");
    if let Ok(s) = fs::read_to_string(&meta) {
        let mut lines: Vec<String> = s.lines().map(|l| l.to_string()).collect();
        // remove existing status/reason/updated
        lines.retain(|l| {
            !l.starts_with("status=") && !l.starts_with("reason=") && !l.starts_with("updated=")
        });
        lines.push(format!("status={}", status));
        if let Some(r) = reason {
            lines.push(format!("reason={}", r));
        }
        lines.push(format!("updated={}", now_iso()));
        let _ = fs::write(meta, lines.join("\n") + "\n");
    }
}

pub fn load_manifest() -> Vec<TxItem> {
    let path = current_tx_dir().join("manifest.json");
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_manifest(items: &[TxItem]) {
    let path = current_tx_dir().join("manifest.json");
    if let Ok(s) = serde_json::to_string_pretty(items) {
        let _ = fs::write(path, s);
    }
}

pub fn tx_record(item: TxItem) {
    let mut items = load_manifest();
    if let Some(existing) = items.iter_mut().find(|i| i.id == item.id) {
        // preserve original old/backup
        let old = existing.old.clone();
        let backup = existing.backup.clone();
        existing.kind = item.kind;
        existing.target = item.target;
        existing.old = old;
        existing.new = item.new;
        existing.backup = backup;
        existing.extra = item.extra;
        existing.state = "recorded".to_string();
    } else {
        let mut it = item;
        it.state = "recorded".to_string();
        items.push(it);
    }
    save_manifest(&items);
}

pub fn tx_update_state(id: &str, state: &str) {
    let mut items = load_manifest();
    if let Some(it) = items.iter_mut().find(|i| i.id == id) {
        it.state = state.to_string();
    }
    save_manifest(&items);
}

#[allow(dead_code)]
pub fn tx_backup_existing(path: &Path, id: &str) -> Option<String> {
    if is_dry_run() {
        return Some(String::new());
    } // dry run: no backup
    if !path.exists() {
        return Some(String::new());
    }
    let dir = current_tx_dir().join("files");
    let _ = fs::create_dir_all(&dir);
    let name = id.replace(['/', ' '], "_");
    let dest = dir.join(&name);
    if fs::copy(path, &dest).is_ok() {
        // meta
        let meta = format!("source={}\n", path.display());
        let _ = fs::write(format!("{}.meta", dest.display()), meta);
        Some(name)
    } else {
        None
    }
}

pub fn rooted_backup_restore(backup: &str, path: &Path) -> Result<()> {
    let src = current_tx_dir().join("files").join(backup);
    if !src.exists() {
        return Err(anyhow::anyhow!("backup missing"));
    }
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    fs::copy(&src, path)?;
    Ok(())
}

pub fn tx_end_cleanup_dryrun() {
    if is_dry_run() {
        let dir = current_tx_dir();
        if dir.to_string_lossy().contains("kernyx-dryrun.") {
            let _ = fs::remove_dir_all(&dir);
        }
    }
    std::env::remove_var("KERNYX_TX_DIR");
    std::env::remove_var("KERNYX_TX_ID");
}

pub fn tx_list() -> Vec<(String, String, String, String)> {
    let mut out = Vec::new();
    let root = tx_root();
    if !root.exists() {
        return out;
    }
    if let Ok(entries) = fs::read_dir(&root) {
        for e in entries.flatten() {
            let dir = e.path();
            if !dir.is_dir() {
                continue;
            }
            let meta = dir.join("meta");
            if !meta.exists() {
                continue;
            }
            let content = fs::read_to_string(&meta).unwrap_or_default();
            let mut status = String::new();
            let mut created = String::new();
            let mut profile = String::new();
            for line in content.lines() {
                if let Some(v) = line.strip_prefix("status=") {
                    status = v.to_string();
                }
                if let Some(v) = line.strip_prefix("created=") {
                    created = v.to_string();
                }
                if let Some(v) = line.strip_prefix("profile=") {
                    profile = v.to_string();
                }
            }
            out.push((
                e.file_name().to_string_lossy().to_string(),
                created,
                profile,
                status,
            ));
        }
    }
    out.sort();
    out
}
