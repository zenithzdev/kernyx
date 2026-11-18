use crate::util::*;
use crate::util::{read_file, rooted};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub fn backup_root() -> std::path::PathBuf {
    crate::tx::state_dir().join("snapshots")
}

pub fn create(label: &str) -> Result<String> {
    let id = format!("snapshot-{}-{}", timestamp_id(), rand_suffix());
    let dir = backup_root().join(&id);
    ensure_dir(&dir)?;
    ensure_dir(&dir.join("files"))?;
    let meta = format!(
        "id={}\ncreated={}\nlabel={}\nhostname={}\nkernel={}\n",
        id,
        now_iso(),
        label,
        read_file(&rooted("/proc/sys/kernel/hostname")),
        read_file(&rooted("/proc/sys/kernel/osrelease"))
    );
    fs::write(dir.join("meta"), meta)?;
    let legacy = crate::persist::legacy_managed();
    let mut files = vec![
        crate::persist::SYSCTL_PATH.to_string(),
        crate::persist::TMPFILES_PATH.to_string(),
        crate::persist::UDEV_PATH.to_string(),
        "/etc/systemd/system/kernyx-persist.service".to_string(),
    ];
    files.extend(
        legacy
            .iter()
            .map(|(path, _)| path.to_string_lossy().to_string()),
    );
    let mut list = String::new();
    for f in &files {
        let p = rooted(f);
        if p.exists() {
            let base = f.trim_start_matches('/');
            if let Some(parent) = Path::new(&format!("{}/files", dir.display()))
                .join(base)
                .parent()
            {
                let _ = ensure_dir(parent);
            }
            let dest = dir.join("files").join(base);
            if fs::copy(&p, &dest).is_ok() {
                list.push_str(f);
                list.push('\n');
            }
        }
    }
    if !list.is_empty() {
        fs::write(dir.join("files.list"), list)?;
    }
    // sysctl values from every KERNYX-managed drop-in
    let mut out = String::new();
    let mut managed: Vec<String> = vec![crate::persist::SYSCTL_PATH.to_string()];
    managed.extend(
        legacy
            .iter()
            .map(|(path, _)| path.to_string_lossy().to_string()),
    );
    for managed_path in &managed {
        let dropin = rooted(managed_path);
        if !dropin.exists() {
            continue;
        }
        for line in fs::read_to_string(&dropin).unwrap_or_default().lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') || t.starts_with("Managed") {
                continue;
            }
            if let Some((k, _)) = t.split_once('=') {
                let key = k.trim();
                let val = sysctl_read(key);
                out.push_str(&format!("{}\t{}\n", key, val));
            }
        }
    }
    if !out.is_empty() {
        fs::write(dir.join("sysctl.tsv"), out)?;
    }
    Ok(id)
}

pub fn list() {
    let root = backup_root();
    if !root.exists() {
        println!("No snapshots.");
        return;
    }
    if let Ok(entries) = fs::read_dir(&root) {
        for e in entries.flatten() {
            println!("{}", e.file_name().to_string_lossy());
        }
    }
}
