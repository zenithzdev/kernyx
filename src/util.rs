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
    let path = std::env::var("PATH").unwrap_or_default();
    for dir in path.split(':') {
        let candidate = Path::new(dir).join(program);
        if candidate.is_file() {
            return true;
        }
    }
    false
}

pub fn now_iso() -> String {
    format_time("%Y-%m-%dT%H:%M:%SZ")
}

pub fn timestamp_id() -> String {
    format_time("%Y%m%dT%H%M%SZ")
}

fn format_time(fmt: &str) -> String {
    use libc::{gmtime_r, strftime, time, tm};
    use std::ffi::CStr;
    unsafe {
        let mut t: libc::time_t = 0;
        time(&mut t);
        let mut tm: tm = std::mem::zeroed();
        if gmtime_r(&t, &mut tm).is_null() {
            return "unknown".to_string();
        }
        let mut buf = [0i8; 64];
        let written = strftime(buf.as_mut_ptr(), buf.len(), fmt.as_ptr() as *const _, &tm);
        if written == 0 {
            return "unknown".to_string();
        }
        CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
    }
}

pub fn rand_suffix() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut h);
    format!("{:x}", h.finish() & 0xffffff)
}

pub fn is_root() -> bool {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            let uid = rest.split_whitespace().next().unwrap_or("1");
            return uid == "0";
        }
    }
    false
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("cannot create {}", path.display()))
}

pub fn sysctl_proc_path(key: &str) -> PathBuf {
    let p = key.replace('.', "/");
    rooted(&format!("/proc/sys/{}", p))
}

pub fn sysctl_read(key: &str) -> String {
    read_file(&sysctl_proc_path(key))
}

pub fn sysctl_write(key: &str, value: &str) -> Result<()> {
    let path = sysctl_proc_path(key);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, format!("{}\n", value)).with_context(|| format!("write {}", path.display()))
}

pub fn normalize_value(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn equivalent(left: &str, right: &str) -> bool {
    normalize_value(left) == normalize_value(right)
}

pub fn write_sysfs(path: &Path, value: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    fs::write(path, format!("{}\n", value)).with_context(|| format!("write {}", path.display()))
}

pub fn sha256_file(path: &Path) -> String {
    if !path.exists() {
        return "missing".to_string();
    }
    use sha2::{Digest, Sha256};
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(_) => return "missing".to_string(),
    };
    let mut hasher = Sha256::new();
    hasher.update(&data);
    format!("{:x}", hasher.finalize())
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
