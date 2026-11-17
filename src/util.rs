use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn fake_root() -> Option<PathBuf> {
    std::env::var("KERNYX_FAKE_ROOT").ok().map(PathBuf::from)
}

pub fn rooted(path: &str) -> PathBuf {
    if let Some(root) = fake_root() {
        if path.starts_with(&root.to_string_lossy().to_string()) {
            PathBuf::from(path)
        } else {
            root.join(path.trim_start_matches('/'))
        }
    } else {
        PathBuf::from(path)
    }
}

pub fn read_file(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub fn read_int_file(path: &Path, default: i64) -> i64 {
    read_file(path)
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

#[allow(dead_code)]
pub fn run_cmd(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {}", program))?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "{} failed: {}",
            program,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn run_cmd_lossy(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

pub fn have(program: &str) -> bool {
    Command::new("command")
        .arg("-v")
        .arg(program)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
        || std::process::Command::new(program)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

pub fn now_iso() -> String {
    // Simple UTC timestamp without chrono dependency.
    let out = run_cmd_lossy("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]);
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

pub fn timestamp_id() -> String {
    let out = run_cmd_lossy("date", &["-u", "+%Y%m%dT%H%M%SZ"]);
    if out.is_empty() {
        "00000000T000000Z".to_string()
    } else {
        out
    }
}

pub fn rand_suffix() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut h);
    format!("{:x}", h.finish() & 0xffffff)
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("cannot create {}", path.display()))
}

pub fn sysctl_proc_path(key: &str) -> PathBuf {
    let p = key.replace('.', "/");
    rooted(&format!("/proc/sys/{}", p))
}

pub fn sysctl_read(key: &str) -> String {
    let out = run_cmd_lossy("sysctl", &["-n", key]);
    if !out.is_empty() {
        return out;
    }
    read_file(&sysctl_proc_path(key))
}

pub fn sysctl_write(key: &str, value: &str) -> Result<()> {
    let status = Command::new("sysctl")
        .arg("-w")
        .arg(format!("{}={}", key, value))
        .status();
    if let Ok(s) = status {
        if s.success() {
            return Ok(());
        }
    }
    // fallback
    let path = sysctl_proc_path(key);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, format!("{}\n", value))?;
    Ok(())
}

pub fn sha256_file(path: &Path) -> String {
    if !path.exists() {
        return "missing".to_string();
    }
    let out = run_cmd_lossy("sha256sum", &[path.to_str().unwrap_or("")]);
    out.split_whitespace()
        .next()
        .unwrap_or("missing")
        .to_string()
}

pub fn atomic_write(path: &Path, content: &str, mode: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let tmp = path.with_extension("tmp.kernyx");
    fs::write(&tmp, content).with_context(|| format!("write {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(mode)).ok();
    }
    fs::rename(&tmp, path).with_context(|| format!("rename {}", path.display()))?;
    Ok(())
}
