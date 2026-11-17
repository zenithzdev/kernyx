use crate::hardware::Hardware;
use crate::util::{read_file, read_int_file, rooted, run_cmd_lossy};
use std::path::Path;

pub fn detect() -> Hardware {
    let mut hw = Hardware::default();
    detect_os(&mut hw);
    detect_cpu(&mut hw);
    detect_mem(&mut hw);
    detect_storage(&mut hw);
    detect_network(&mut hw);
    detect_capabilities(&mut hw);
    hw.profile_id = "automatic".to_string();
    hw
}

fn detect_os(hw: &mut Hardware) {
    let osr = rooted("/etc/os-release");
    if let Ok(s) = std::fs::read_to_string(&osr) {
        for line in s.lines() {
            if let Some(v) = line.strip_prefix("ID=") {
                hw.os_id = v.trim_matches('"').to_string();
            }
            if let Some(v) = line.strip_prefix("NAME=") {
                hw.os_name = v.trim_matches('"').to_string();
            }
            if let Some(v) = line.strip_prefix("VERSION_ID=") {
                hw.os_version = v.trim_matches('"').to_string();
            }
            if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
                hw.os_pretty = v.trim_matches('"').to_string();
            }
        }
    }
    hw.os_kernel = run_cmd_lossy("uname", &["-r"]);
    hw.os_arch = run_cmd_lossy("uname", &["-m"]);
    hw.os_supported = hw.os_id == "ubuntu" || hw.os_id == "debian";
    // pve
    if rooted("/etc/pve").exists() || std::path::Path::new("/usr/bin/pveversion").exists() {
        hw.os_pve = true;
    }
    // virt
    let virt = run_cmd_lossy("systemd-detect-virt", &[]);
    hw.os_virt = if virt == "none" { String::new() } else { virt };
    hw.os_is_vm = !hw.os_virt.is_empty();
    // cloud
    let vendor = read_file(&rooted("/sys/class/dmi/id/sys_vendor"));
    let board = read_file(&rooted("/sys/class/dmi/id/board_vendor"));
    let product = read_file(&rooted("/sys/class/dmi/id/product_name"));
    let hv = read_file(&rooted("/sys/hypervisor/uuid"));
    if vendor.contains("Amazon") || board.contains("Amazon") || hv.to_lowercase().starts_with("ec2")
    {
        hw.os_cloud = "aws".to_string();
    } else if vendor.contains("Google") || product.contains("Google") {
        hw.os_cloud = "gcp".to_string();
    } else if vendor.contains("Microsoft") {
        hw.os_cloud = "azure".to_string();
    }
    hw.os_is_container = std::env::var("container").is_ok();
    // systemd
    hw.os_systemd_running = run_cmd_lossy("systemctl", &["is-system-running"]).contains("running")
        || run_cmd_lossy("systemctl", &["is-system-running"]) == "degraded";
    hw.is_virtual = hw.os_is_vm || hw.os_is_container;
    hw.set("os.id", hw.os_id.clone());
    hw.set("os.pve", if hw.os_pve { "1" } else { "0" }.to_string());
    hw.set("os.cloud", hw.os_cloud.clone());
    hw.set(
        "cap.is_virtual",
        if hw.is_virtual { "1" } else { "0" }.to_string(),
    );
    hw.set(
        "cap.is_cloud",
        if !hw.os_cloud.is_empty() { "1" } else { "0" }.to_string(),
    );
    hw.set(
        "cap.is_aws",
        if hw.os_cloud == "aws" { "1" } else { "0" }.to_string(),
    );
    hw.set("cap.is_pve", if hw.os_pve { "1" } else { "0" }.to_string());
}

fn detect_cpu(hw: &mut Hardware) {
    // model
    let cpuinfo = read_file(Path::new("/proc/cpuinfo"));
    for line in cpuinfo.lines() {
        if let Some(v) = line.strip_prefix("model name") {
            if let Some(val) = v.split(':').nth(1) {
                hw.cpu_model = val.trim().to_string();
                break;
            }
        }
    }
    if hw.cpu_model.is_empty() {
        hw.cpu_model = "unknown".to_string();
    }
    let nproc = run_cmd_lossy("nproc", &[]);
    let effective: i64 = nproc.parse().unwrap_or(1);
    hw.cpu_effective = effective;
    hw.cpu_physical = effective; // simplified
                                 // numa
                                 // leave 1
    hw.set("cpu.model", hw.cpu_model.clone());
    hw.set("cap.cpu_effective", hw.cpu_effective.to_string());
    hw.set("cpu.effective", hw.cpu_effective.to_string());
    hw.set(
        "cpu.governors",
        read_file(&rooted(
            "/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors",
        )),
    );
    hw.set(
        "cpu.governor",
        read_file(&rooted(
            "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
        )),
    );
    hw.set(
        "cpu.scaling_driver",
        read_file(&rooted(
            "/sys/devices/system/cpu/cpu0/cpufreq/scaling_driver",
        )),
    );
    hw.set(
        "cap.has_cpufreq",
        if rooted("/sys/devices/system/cpu/cpufreq").exists() {
            "1"
        } else {
            "0"
        }
        .to_string(),
    );
    hw.set(
        "kernel.sched_autogroup_enabled",
        read_file(Path::new("/proc/sys/kernel/sched_autogroup_enabled")),
    );
    hw.set("cap.ram_mb", hw.ram_mb.to_string()); // placeholder
}

fn detect_mem(hw: &mut Hardware) {
    let meminfo = read_file(Path::new("/proc/meminfo"));
    for line in meminfo.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kb: i64 = rest
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            hw.ram_mb = kb / 1024;
        }
        if let Some(rest) = line.strip_prefix("SwapTotal:") {
            let kb: i64 = rest
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            hw.swap_mb = kb / 1024;
        }
    }
    hw.set("cap.ram_mb", hw.ram_mb.to_string());
    hw.set("ram.swap_total_mb", hw.swap_mb.to_string());
    hw.set(
        "cap.has_swap",
        if hw.swap_mb > 0 { "1" } else { "0" }.to_string(),
    );
    hw.set("ram.pressure_some_avg10", "0.00".to_string());
    hw.set(
        "ram.file_max",
        read_file(Path::new("/proc/sys/fs/file-max")),
    );
    hw.set("ram.nr_open", read_file(Path::new("/proc/sys/fs/nr_open")));
    hw.set(
        "ram.pid_max",
        read_file(Path::new("/proc/sys/kernel/pid_max")),
    );
    hw.set(
        "ram.max_map_count",
        read_file(Path::new("/proc/sys/vm/max_map_count")),
    );
    hw.set(
        "ram.vfs_cache_pressure",
        read_file(Path::new("/proc/sys/vm/vfs_cache_pressure")),
    );
    hw.set(
        "ram.dirty_background_ratio",
        read_file(Path::new("/proc/sys/vm/dirty_background_ratio")),
    );
    hw.set(
        "ram.dirty_ratio",
        read_file(Path::new("/proc/sys/vm/dirty_ratio")),
    );
    hw.set(
        "ram.dirty_background_bytes",
        read_file(Path::new("/proc/sys/vm/dirty_background_bytes")),
    );
    hw.set(
        "ram.dirty_bytes",
        read_file(Path::new("/proc/sys/vm/dirty_bytes")),
    );
    hw.set(
        "ram.swappiness",
        read_file(Path::new("/proc/sys/vm/swappiness")),
    );
    // zram
    let zram = run_cmd_lossy("zramctl", &[]);
    hw.zram_active = !zram.is_empty();
    hw.set(
        "cap.zram_active",
        if hw.zram_active { "1" } else { "0" }.to_string(),
    );
    hw.set("ram.thp_mode", "madvise".to_string());
}

fn detect_storage(hw: &mut Hardware) {
    // root disk
    let mut root_dev = run_cmd_lossy("findmnt", &["-no", "SOURCE", "/"]);
    if root_dev.is_empty() {
        let mounts = read_file(Path::new("/proc/mounts"));
        for line in mounts.lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 2 && cols[1] == "/" {
                root_dev = cols[0].to_string();
                break;
            }
        }
    }
    // normalize /dev/nvme0n1p1 -> nvme0n1
    let mut devname = root_dev.trim_start_matches("/dev/").to_string();
    if devname.starts_with("nvme") {
        if let Some(idx) = devname.find('p') {
            if devname[..idx].chars().all(|c| c.is_alphanumeric()) {
                devname = devname[..idx].to_string();
            }
        }
    } else {
        devname = devname
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .to_string();
    }
    let rotational_path = format!("/sys/block/{}/queue/rotational", devname);
    let rotational = read_file(Path::new(&rotational_path));
    let is_nvme = devname.starts_with("nvme");
    if hw.os_cloud == "aws" && is_nvme {
        hw.storage_class = "ebs".to_string();
    } else if is_nvme {
        hw.storage_class = "nvme".to_string();
    } else if rotational == "0" {
        hw.storage_class = "ssd".to_string();
    } else {
        hw.storage_class = "hdd".to_string();
    }
    if devname.starts_with("vd") || devname.starts_with("xvd") {
        hw.storage_class = "virtio".to_string();
    }
    hw.has_nvme = hw.storage_class == "nvme";
    hw.has_ssd = hw.storage_class == "ssd";
    hw.has_hdd = hw.storage_class == "hdd";
    hw.has_virtio = hw.storage_class == "virtio";
    hw.has_ebs = hw.storage_class == "ebs";
    hw.set(
        "cap.has_nvme",
        if hw.has_nvme { "1" } else { "0" }.to_string(),
    );
    hw.set(
        "cap.has_ssd",
        if hw.has_ssd { "1" } else { "0" }.to_string(),
    );
    hw.set(
        "cap.has_hdd",
        if hw.has_hdd { "1" } else { "0" }.to_string(),
    );
    hw.set(
        "cap.has_virtio",
        if hw.has_virtio { "1" } else { "0" }.to_string(),
    );
    hw.set(
        "cap.has_ebs",
        if hw.has_ebs { "1" } else { "0" }.to_string(),
    );
    hw.set("cap.disk_types", hw.storage_class.clone());
}

fn detect_network(hw: &mut Hardware) {
    // default interface
    let route = run_cmd_lossy("ip", &["route", "show", "default"]);
    let mut iface = String::new();
    // find "dev X"
    let mut it = route.split_whitespace();
    while let Some(t) = it.next() {
        if t == "dev" {
            iface = it.next().unwrap_or("").to_string();
            break;
        }
    }
    if iface.is_empty() {
        iface = "eth0".to_string();
    }
    hw.set("net.default_if", iface.clone());
    // speed
    let speed = read_int_file(&rooted(&format!("/sys/class/net/{}/speed", iface)), 0);
    let speed = if speed <= 0 { 1000 } else { speed };
    hw.max_nic_mbps = speed as i64;
    hw.set("cap.max_nic_mbps", hw.max_nic_mbps.to_string());
    // tcp
    hw.set(
        "net.tcp_congestion_control",
        read_file(Path::new("/proc/sys/net/ipv4/tcp_congestion_control")),
    );
    let avail = read_file(Path::new(
        "/proc/sys/net/ipv4/tcp_available_congestion_control",
    ));
    hw.set("net.tcp_available_congestion_control", avail.clone());
    hw.set(
        "kernel.bbr",
        if avail.contains("bbr") { "1" } else { "0" }.to_string(),
    );
    hw.set(
        "cap.has_bbr",
        if avail.contains("bbr") { "1" } else { "0" }.to_string(),
    );
    hw.set(
        "net.default_qdisc",
        read_file(Path::new("/proc/sys/net/core/default_qdisc")),
    );
    let fq = if Path::new("/sys/module/sch_fq").exists()
        || run_cmd_lossy("modinfo", &["sch_fq"]).contains("filename:")
    {
        "1"
    } else {
        "0"
    };
    hw.set("cap.has_fq", fq.to_string());
    hw.set("kernel.fq", fq.to_string());
    hw.set(
        "net.rmem_max",
        read_file(Path::new("/proc/sys/net/core/rmem_max")),
    );
    hw.set(
        "net.wmem_max",
        read_file(Path::new("/proc/sys/net/core/wmem_max")),
    );
    hw.set(
        "net.tcp_rmem",
        read_file(Path::new("/proc/sys/net/ipv4/tcp_rmem")),
    );
    hw.set(
        "net.tcp_wmem",
        read_file(Path::new("/proc/sys/net/ipv4/tcp_wmem")),
    );
    hw.set(
        "net.somaxconn",
        read_file(Path::new("/proc/sys/net/core/somaxconn")),
    );
    hw.set(
        "net.netdev_max_backlog",
        read_file(Path::new("/proc/sys/net/core/netdev_max_backlog")),
    );
    hw.set(
        "net.tcp_max_syn_backlog",
        read_file(Path::new("/proc/sys/net/ipv4/tcp_max_syn_backlog")),
    );
    hw.set(
        "net.tcp_mtu_probing",
        read_file(Path::new("/proc/sys/net/ipv4/tcp_mtu_probing")),
    );
    hw.set(
        "net.tcp_slow_start_after_idle",
        read_file(Path::new("/proc/sys/net/ipv4/tcp_slow_start_after_idle")),
    );
    hw.set(
        "net.tcp_tw_reuse",
        read_file(Path::new("/proc/sys/net/ipv4/tcp_tw_reuse")),
    );
    // fq availability
    hw.set(
        "cap.has_fq",
        if run_cmd_lossy("tc", &["qdisc", "show"]).contains("fq")
            || std::path::Path::new("/proc/net/psched").exists()
        {
            "1"
        } else {
            "0"
        }
        .to_string(),
    );
    // physical rx queues
    hw.set("cap.rx_queues_physical", "0".to_string());
}

fn detect_capabilities(hw: &mut Hardware) {
    hw.set("cap.ram_mb", hw.ram_mb.to_string());
    hw.set("cap.cpu_effective", hw.cpu_effective.to_string());
    hw.set(
        "cap.is_virtual",
        if hw.is_virtual { "1" } else { "0" }.to_string(),
    );
    hw.set("cap.numa_nodes", "1".to_string());
    hw.set("cap.smt", "1".to_string());
    hw.set(
        "os.systemd_running",
        if hw.os_systemd_running { "1" } else { "0" }.to_string(),
    );
    hw.set(
        "os.supported",
        if hw.os_supported { "1" } else { "0" }.to_string(),
    );
    hw.set(
        "kernel.bbr",
        hw.get("kernel.bbr")
            .cloned()
            .unwrap_or_else(|| "0".to_string()),
    );
}
