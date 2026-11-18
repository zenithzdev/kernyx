use crate::hardware::Hardware;
use crate::util::{read_file, read_int_file, rooted, sysctl_proc_path};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub fn detect() -> Hardware {
    let mut hw = Hardware::default();
    detect_os(&mut hw);
    detect_cpu(&mut hw);
    detect_mem(&mut hw);
    detect_storage(&mut hw);
    detect_network(&mut hw);
    detect_workload(&mut hw);
    detect_capabilities(&mut hw);
    hw.profile_id = "automatic".to_string();
    hw
}

fn sysctl(key: &str) -> String {
    let direct = read_file(&sysctl_proc_path(key));
    if !direct.is_empty() {
        return direct;
    }
    crate::util::sysctl_read(key)
}

fn read_choice(path: &str) -> String {
    let raw = read_file(&rooted(path));
    if let Some(start) = raw.find('[') {
        if let Some(end) = raw[start + 1..].find(']') {
            return raw[start + 1..start + 1 + end].to_string();
        }
    }
    raw
}

fn parse_psi(path: &str) -> f64 {
    let raw = read_file(&rooted(path));
    for line in raw.lines() {
        if line.starts_with("some ") {
            for field in line.split_whitespace() {
                if let Some(v) = field.strip_prefix("avg10=") {
                    return v.parse().unwrap_or(0.0);
                }
            }
        }
    }
    0.0
}

fn is_container() -> bool {
    if rooted("/.dockerenv").exists() || rooted("/run/.containerenv").exists() {
        return true;
    }
    if std::env::var("container").is_ok() {
        return true;
    }
    if rooted("/proc/vz").exists() {
        return true;
    }
    let cgroup = read_file(&rooted("/proc/1/cgroup"));
    if cgroup.contains("docker")
        || cgroup.contains("kubepods")
        || cgroup.contains("containerd")
        || cgroup.contains("lxc")
        || cgroup.contains("podman")
    {
        return true;
    }
    // A container usually has a bind-mounted /proc/1/root distinct from /.
    false
}

fn writable(path: &str) -> bool {
    std::fs::OpenOptions::new()
        .write(true)
        .open(rooted(path))
        .is_ok()
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
    if hw.os_pretty.is_empty() {
        hw.os_pretty = format!("{} {}", hw.os_name, hw.os_version)
            .trim()
            .to_string();
    }
    hw.os_kernel = read_file(&rooted("/proc/sys/kernel/osrelease"));
    if hw.os_kernel.is_empty() {
        let version = read_file(&rooted("/proc/version"));
        hw.os_kernel = version.split_whitespace().nth(2).unwrap_or("").to_string();
    }
    hw.os_arch = std::env::consts::ARCH.to_string();

    hw.os_pve = rooted("/etc/pve").exists() || rooted("/usr/bin/pveversion").exists();

    hw.os_is_container = is_container();
    hw.os_virt = String::new();
    if hw.os_virt.is_empty() {
        let product = read_file(&rooted("/sys/class/dmi/id/product_name"));
        let vendor = read_file(&rooted("/sys/class/dmi/id/sys_vendor"));
        let probe = format!("{} {}", product, vendor).to_lowercase();
        if probe.contains("kvm") {
            hw.os_virt = "kvm".to_string();
        } else if probe.contains("vmware") {
            hw.os_virt = "vmware".to_string();
        } else if probe.contains("virtualbox") {
            hw.os_virt = "oracle".to_string();
        } else if probe.contains("amazon ec2") || probe.contains("amazon") {
            hw.os_virt = "amazon".to_string();
        } else if probe.contains("microsoft") {
            hw.os_virt = "microsoft".to_string();
        } else if probe.contains("google") {
            hw.os_virt = "google".to_string();
        }
    }
    if hw.os_is_container && hw.os_virt.is_empty() {
        hw.os_virt = "container".to_string();
    }
    hw.os_is_vm = !hw.os_virt.is_empty() && !hw.os_is_container;

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
    } else if vendor.contains("DigitalOcean") {
        hw.os_cloud = "digitalocean".to_string();
    } else if vendor.contains("Hetzner") {
        hw.os_cloud = "hetzner".to_string();
    }

    hw.os_systemd_running = rooted("/run/systemd/system").exists();
    hw.os_supported = matches!(
        hw.os_id.as_str(),
        "ubuntu"
            | "debian"
            | "linuxmint"
            | "pop"
            | "raspbian"
            | "fedora"
            | "rhel"
            | "centos"
            | "rocky"
            | "almalinux"
            | "arch"
            | "manjaro"
            | "alpine"
            | "opensuse"
            | "sles"
    ) || rooted("/proc/sys/vm/swappiness").exists();

    let mounts = read_file(&rooted("/proc/mounts"));
    hw.read_only_root = mounts.lines().any(|line| {
        let cols: Vec<&str> = line.split_whitespace().collect();
        cols.len() >= 4 && cols[1] == "/" && cols[3].split(',').any(|o| o == "ro")
    });
    hw.kernel_cmdline = read_file(&rooted("/proc/cmdline"));
    hw.is_virtual = hw.os_is_vm || hw.os_is_container;

    hw.set("os.id", hw.os_id.clone());
    hw.set("os.pve", if hw.os_pve { "1" } else { "0" });
    hw.set("os.cloud", hw.os_cloud.clone());
    hw.set("os.virt", hw.os_virt.clone());
    hw.set("cap.is_virtual", if hw.is_virtual { "1" } else { "0" });
    hw.set(
        "cap.is_cloud",
        if !hw.os_cloud.is_empty() { "1" } else { "0" },
    );
    hw.set("cap.is_aws", if hw.os_cloud == "aws" { "1" } else { "0" });
    hw.set("cap.is_pve", if hw.os_pve { "1" } else { "0" });
}

fn detect_cpu(hw: &mut Hardware) {
    let cpuinfo = read_file(&rooted("/proc/cpuinfo"));
    let mut vendor = String::new();
    let mut model = String::new();
    let mut sockets = BTreeSet::new();
    let mut cores = BTreeSet::new();
    let mut threads = 0i64;

    for block in cpuinfo.split("\n\n") {
        let mut processor = false;
        let mut socket = String::new();
        let mut core = String::new();
        for line in block.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "processor" => {
                    processor = true;
                    threads += 1;
                }
                "vendor_id" | "CPU implementer" if vendor.is_empty() => vendor = value.to_string(),
                "model name" | "Model" | "Hardware" if model.is_empty() => {
                    model = value.to_string()
                }
                "physical id" => socket = value.to_string(),
                "core id" => core = value.to_string(),
                _ => {}
            }
        }
        if processor {
            if !socket.is_empty() {
                sockets.insert(socket.clone());
            }
            if !core.is_empty() {
                cores.insert((socket.clone(), core));
            }
        }
    }

    if let Ok(rd) = std::fs::read_dir(rooted("/sys/devices/system/cpu")) {
        let mut sys_threads = 0i64;
        let mut sys_cores = BTreeSet::new();
        let mut sys_sockets = BTreeSet::new();
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("cpu")
                || name[3..].chars().any(|c| !c.is_ascii_digit())
                || name == "cpuidle"
            {
                continue;
            }
            sys_threads += 1;
            let topo = format!("/sys/devices/system/cpu/{}/topology", name);
            let core = read_file(&rooted(&format!("{}/core_id", topo)));
            let socket = read_file(&rooted(&format!("{}/physical_package_id", topo)));
            if !core.is_empty() {
                sys_cores.insert((socket.clone(), core));
            }
            if !socket.is_empty() {
                sys_sockets.insert(socket);
            }
        }
        if sys_threads > 0 {
            threads = sys_threads;
            if !sys_cores.is_empty() {
                cores = sys_cores;
            }
            if !sys_sockets.is_empty() {
                sockets = sys_sockets;
            }
        }
    }

    let effective: i64 = threads.max(1);
    hw.cpu_effective = effective;
    hw.cpu_vendor = if vendor.is_empty() {
        "unknown".to_string()
    } else {
        vendor
    };
    hw.cpu_model = if model.is_empty() {
        hw.cpu_vendor.clone()
    } else {
        model
    };
    hw.cpu_sockets = sockets.len().max(1) as i64;
    hw.cpu_cores = if cores.is_empty() {
        threads.max(1)
    } else {
        cores.len() as i64
    };
    if hw.cpu_cores <= 0 || hw.cpu_cores > effective {
        hw.cpu_cores = effective;
    }
    hw.cpu_physical = hw.cpu_cores;
    hw.cpu_smt = effective > hw.cpu_cores;

    let node_root = rooted("/sys/devices/system/node");
    hw.numa_nodes = std::fs::read_dir(&node_root)
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.starts_with("node") && name[4..].chars().all(|c| c.is_ascii_digit())
                })
                .count()
                .max(1) as i64
        })
        .unwrap_or(1);
    hw.numa_online = read_file(&node_root.join("online"));

    if let Some(base) = cpufreq_base() {
        hw.has_cpufreq = true;
        hw.cpu_driver = read_file(&base.join("scaling_driver"));
        hw.cpu_governor = read_file(&base.join("scaling_governor"));
        hw.cpu_governors = read_file(&base.join("scaling_available_governors"));
        hw.cpu_epp = read_file(&base.join("energy_performance_preference"));
        hw.cpu_epp_available = read_file(&base.join("energy_performance_available_preferences"));
    }

    hw.set("cpu.model", hw.cpu_model.clone());
    hw.set("cpu.vendor", hw.cpu_vendor.clone());
    hw.set("cap.cpu_effective", hw.cpu_effective.to_string());
    hw.set("cpu.effective", hw.cpu_effective.to_string());
    hw.set("cpu.cores", hw.cpu_cores.to_string());
    hw.set("cpu.threads", hw.cpu_effective.to_string());
    hw.set("cpu.sockets", hw.cpu_sockets.to_string());
    hw.set("cpu.smt", if hw.cpu_smt { "1" } else { "0" });
    hw.set("cap.numa_nodes", hw.numa_nodes.to_string());
    hw.set("cap.has_cpufreq", if hw.has_cpufreq { "1" } else { "0" });
    hw.set("cpu.governors", hw.cpu_governors.clone());
    hw.set("cpu.governor", hw.cpu_governor.clone());
    hw.set("cpu.scaling_driver", hw.cpu_driver.clone());
    hw.set("cpu.epp", hw.cpu_epp.clone());
    hw.set(
        "kernel.sched_autogroup_enabled",
        sysctl("kernel.sched_autogroup_enabled"),
    );
    hw.numa_balancing = sysctl("kernel.numa_balancing").parse().unwrap_or(0);
}

fn cpufreq_base() -> Option<PathBuf> {
    for i in 0..=255 {
        let p = PathBuf::from(format!("/sys/devices/system/cpu/cpu{i}/cpufreq"));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn meminfo() -> BTreeMap<String, i64> {
    let mut map = BTreeMap::new();
    for line in read_file(&rooted("/proc/meminfo")).lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let value = rest
            .split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        map.insert(key.trim().to_string(), value);
    }
    map
}

fn detect_mem(hw: &mut Hardware) {
    let mem = meminfo();
    hw.ram_mb = mem.get("MemTotal").copied().unwrap_or(0) / 1024;
    hw.mem_available_mb = mem.get("MemAvailable").copied().unwrap_or(0) / 1024;
    hw.swap_mb = mem.get("SwapTotal").copied().unwrap_or(0) / 1024;
    hw.swap_free_mb = mem.get("SwapFree").copied().unwrap_or(0) / 1024;
    hw.pressure_memory = parse_psi("/proc/pressure/memory");
    hw.pressure_cpu = parse_psi("/proc/pressure/cpu");
    hw.pressure_io = parse_psi("/proc/pressure/io");

    hw.zram_active = std::fs::read_dir(rooted("/sys/block"))
        .map(|rd| {
            rd.flatten().any(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with("zram") && read_int_file(&e.path().join("disksize"), 0) > 0
            })
        })
        .unwrap_or(false);
    let zswap = read_file(&rooted("/sys/module/zswap/parameters/enabled"));
    hw.zswap_enabled = zswap == "Y" || zswap == "1" || zswap == "yes";

    hw.thp_enabled = read_file(&rooted("/sys/kernel/mm/transparent_hugepage/enabled"));
    hw.thp_mode = read_choice("/sys/kernel/mm/transparent_hugepage/enabled");
    hw.thp_defrag = read_choice("/sys/kernel/mm/transparent_hugepage/defrag");
    hw.overcommit_memory = sysctl("vm.overcommit_memory").parse().unwrap_or(0);
    hw.inotify_watches = sysctl("fs.inotify.max_user_watches").parse().unwrap_or(0);
    hw.inotify_instances = sysctl("fs.inotify.max_user_instances").parse().unwrap_or(0);
    hw.aio_max_nr = sysctl("fs.aio-max-nr").parse().unwrap_or(0);

    for (k, v) in [
        ("ram.swap_total_mb", hw.swap_mb),
        ("ram.swap_free_mb", hw.swap_free_mb),
        ("cap.ram_mb", hw.ram_mb),
        ("ram.available_mb", hw.mem_available_mb),
        ("ram.pressure_some_avg10", hw.pressure_memory as i64),
        ("ram.overcommit_memory", hw.overcommit_memory),
        ("ram.inotify_watches", hw.inotify_watches),
        ("ram.inotify_instances", hw.inotify_instances),
        ("ram.aio_max_nr", hw.aio_max_nr),
    ] {
        hw.set(k, v.to_string());
    }
    for (k, key) in [
        ("ram.file_max", "fs.file-max"),
        ("ram.nr_open", "fs.nr_open"),
        ("ram.pid_max", "kernel.pid_max"),
        ("ram.max_map_count", "vm.max_map_count"),
        ("ram.vfs_cache_pressure", "vm.vfs_cache_pressure"),
        ("ram.dirty_background_ratio", "vm.dirty_background_ratio"),
        ("ram.dirty_ratio", "vm.dirty_ratio"),
        ("ram.dirty_background_bytes", "vm.dirty_background_bytes"),
        ("ram.dirty_bytes", "vm.dirty_bytes"),
        ("ram.dirty_expire_centisecs", "vm.dirty_expire_centisecs"),
        ("ram.swappiness", "vm.swappiness"),
        ("ram.min_free_kbytes", "vm.min_free_kbytes"),
        ("kernel.numa_balancing", "kernel.numa_balancing"),
    ] {
        hw.set(k, sysctl(key));
    }
    hw.set("cap.has_swap", if hw.swap_mb > 0 { "1" } else { "0" });
    hw.set("cap.zram_active", if hw.zram_active { "1" } else { "0" });
    hw.set("ram.thp_mode", hw.thp_mode.clone());
    hw.set("ram.thp_enabled", hw.thp_enabled.clone());
}

fn detect_storage(hw: &mut Hardware) {
    let mut source = String::new();
    let mut fstype = String::new();
    let mut options = String::new();
    for line in read_file(&rooted("/proc/mounts")).lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 4 && cols[1] == "/" {
            source = cols[0].to_string();
            fstype = cols[2].to_string();
            options = cols[3].to_string();
            break;
        }
    }
    if source.is_empty() {
        for line in read_file(&rooted("/proc/mounts")).lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 4 && cols[1] == "/" {
                source = cols[0].to_string();
                fstype = cols[2].to_string();
                options = cols[3].to_string();
                break;
            }
        }
    }

    hw.storage_source = source.clone();
    hw.storage_fs = fstype.clone();
    hw.storage_mount_opts = options.clone();
    hw.storage_device = resolve_disk(&source);

    let queue = format!("/sys/block/{}/queue", hw.storage_device);
    let rotational = read_int_file(&rooted(&format!("{queue}/rotational")), 0);
    let is_nvme = hw.storage_device.starts_with("nvme");
    let is_virtio = hw.storage_device.starts_with("vd")
        || hw.storage_device.starts_with("xvd")
        || hw.storage_device.starts_with("virtio");
    if hw.os_cloud == "aws" && is_nvme {
        hw.storage_class = "ebs".to_string();
    } else if is_nvme {
        hw.storage_class = "nvme".to_string();
    } else if is_virtio {
        hw.storage_class = "virtio".to_string();
    } else if rotational == 0 && !hw.storage_device.is_empty() {
        hw.storage_class = "ssd".to_string();
    } else if rotational == 1 {
        hw.storage_class = "hdd".to_string();
    } else {
        hw.storage_class = "unknown".to_string();
    }
    hw.has_nvme = hw.storage_class == "nvme";
    hw.has_ssd = hw.storage_class == "ssd";
    hw.has_hdd = hw.storage_class == "hdd";
    hw.has_virtio = hw.storage_class == "virtio";
    hw.has_ebs = hw.storage_class == "ebs";

    let scheduler_raw = read_file(&rooted(&format!("{queue}/scheduler")));
    hw.storage_scheduler = current_choice(&scheduler_raw);
    hw.storage_schedulers = scheduler_raw
        .replace(['[', ']'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    hw.storage_read_ahead_kb = read_int_file(&rooted(&format!("{queue}/read_ahead_kb")), 0);
    hw.storage_nr_requests = read_int_file(&rooted(&format!("{queue}/nr_requests")), 0);
    hw.storage_discard = read_int_file(&rooted(&format!("{queue}/discard_max_bytes")), 0) > 0;

    for (k, v) in [
        ("cap.has_nvme", hw.has_nvme),
        ("cap.has_ssd", hw.has_ssd),
        ("cap.has_hdd", hw.has_hdd),
        ("cap.has_virtio", hw.has_virtio),
        ("cap.has_ebs", hw.has_ebs),
    ] {
        hw.set(k, if v { "1" } else { "0" });
    }
    hw.set("cap.disk_types", hw.storage_class.clone());
    hw.set("storage.device", hw.storage_device.clone());
    hw.set("storage.source", hw.storage_source.clone());
    hw.set("storage.fs", hw.storage_fs.clone());
    hw.set("storage.mount_opts", hw.storage_mount_opts.clone());
    hw.set("storage.scheduler", hw.storage_scheduler.clone());
    hw.set("storage.schedulers", hw.storage_schedulers.clone());
    hw.set(
        "storage.read_ahead_kb",
        hw.storage_read_ahead_kb.to_string(),
    );
    hw.set("storage.nr_requests", hw.storage_nr_requests.to_string());
    hw.set(
        "storage.discard",
        if hw.storage_discard { "1" } else { "0" },
    );
}

fn current_choice(raw: &str) -> String {
    if let Some(start) = raw.find('[') {
        if let Some(end) = raw[start + 1..].find(']') {
            return raw[start + 1..start + 1 + end].to_string();
        }
    }
    raw.split_whitespace().next().unwrap_or("").to_string()
}

fn resolve_disk(source: &str) -> String {
    // Resolve a mount source such as /dev/root or /dev/nvme0n1p1 to its
    // underlying disk name using sysfs, without shelling out to lsblk.
    let path = std::path::Path::new(source);
    if let Ok(meta) = std::fs::metadata(path) {
        use std::os::unix::fs::MetadataExt;
        let rdev = meta.rdev();
        if rdev != 0 {
            let major = (rdev >> 8) & 0xfff;
            let minor = rdev & 0xff;
            let sys = format!("/sys/dev/block/{}:{}", major, minor);
            if let Ok(link) = std::fs::read_link(rooted(&sys)) {
                let name = link
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !name.is_empty() {
                    let disk = if name.starts_with("nvme") {
                        if let Some(pos) = name.rfind('p') {
                            let suffix = &name[pos + 1..];
                            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
                                name[..pos].to_string()
                            } else {
                                name.clone()
                            }
                        } else {
                            name.clone()
                        }
                    } else {
                        name.trim_end_matches(|c: char| c.is_ascii_digit())
                            .to_string()
                    };
                    if !disk.is_empty() {
                        return disk;
                    }
                }
            }
        }
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.starts_with("nvme") {
        if let Some(pos) = name.rfind('p') {
            let suffix = &name[pos + 1..];
            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
                name[..pos].to_string()
            } else {
                name
            }
        } else {
            name
        }
    } else {
        name.trim_end_matches(|c: char| c.is_ascii_digit())
            .to_string()
    }
}

fn detect_network(hw: &mut Hardware) {
    let route = String::new();
    let mut iface = String::new();
    let mut it = route.split_whitespace();
    while let Some(t) = it.next() {
        if t == "dev" {
            iface = it.next().unwrap_or("").to_string();
            break;
        }
    }
    if iface.is_empty() {
        for line in read_file(&rooted("/proc/net/route")).lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() > 1 && cols[1] == "00000000" {
                iface = cols[0].to_string();
                break;
            }
        }
    }
    if iface.is_empty() {
        if let Ok(rd) = std::fs::read_dir(rooted("/sys/class/net")) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name != "lo" {
                    iface = name;
                    break;
                }
            }
        }
    }
    hw.nic_iface = iface;
    if !hw.nic_iface.is_empty() {
        let base = format!("/sys/class/net/{}", hw.nic_iface);
        let mut speed = read_int_file(&rooted(&format!("{base}/speed")), 0);
        if speed <= 0 {
            speed = 1000;
        }
        hw.max_nic_mbps = if speed > 0 { speed } else { 1000 };

        let driver_link = rooted(&format!("{base}/device/driver"));
        hw.nic_driver = std::fs::read_link(&driver_link)
            .ok()
            .and_then(|target| target.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| {
                driver_link
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            });
        if hw.nic_driver.is_empty() {
            // Driver name unavailable from sysfs; keep empty.
        }

        if let Ok(rd) = std::fs::read_dir(rooted(&format!("{base}/queues"))) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("rx-") {
                    hw.nic_rx_queues += 1;
                } else if name.starts_with("tx-") {
                    hw.nic_tx_queues += 1;
                }
            }
        }
        if let Ok(rd) = std::fs::read_dir(rooted(&format!("{base}/queues"))) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("rx-") {
                    let mask = read_file(&entry.path().join("rps_cpus"));
                    if mask.chars().any(|c| c.is_ascii_hexdigit() && c != '0') {
                        hw.nic_rps_enabled = true;
                        if hw.nic_rps_cpus.is_empty() {
                            hw.nic_rps_cpus = mask;
                        }
                    }
                }
                if name.starts_with("tx-") && hw.nic_xps_cpus.is_empty() {
                    hw.nic_xps_cpus = read_file(&entry.path().join("xps_cpus"));
                }
            }
        }
    }

    hw.tcp_cc = sysctl("net.ipv4.tcp_congestion_control");
    hw.tcp_available_cc = sysctl("net.ipv4.tcp_available_congestion_control");
    hw.default_qdisc = sysctl("net.core.default_qdisc");
    hw.has_bbr = hw.tcp_available_cc.split_whitespace().any(|c| c == "bbr")
        || rooted("/sys/module/tcp_bbr").exists();
    hw.has_fq = rooted("/sys/module/sch_fq").exists();
    hw.conntrack_max = sysctl("net.netfilter.nf_conntrack_max")
        .parse()
        .unwrap_or(0);
    hw.conntrack_count = sysctl("net.netfilter.nf_conntrack_count")
        .parse()
        .unwrap_or(0);
    hw.conntrack = hw.conntrack_max > 0 || rooted("/proc/net/nf_conntrack").exists();

    for (k, v) in [
        ("cap.max_nic_mbps", hw.max_nic_mbps),
        ("cap.nic_rx_queues", hw.nic_rx_queues),
        ("cap.nic_tx_queues", hw.nic_tx_queues),
        ("net.conntrack_max", hw.conntrack_max),
        ("net.conntrack_count", hw.conntrack_count),
    ] {
        hw.set(k, v.to_string());
    }
    for (k, key) in [
        (
            "net.tcp_congestion_control",
            "net.ipv4.tcp_congestion_control",
        ),
        (
            "net.tcp_available_congestion_control",
            "net.ipv4.tcp_available_congestion_control",
        ),
        ("net.default_qdisc", "net.core.default_qdisc"),
        ("net.rmem_max", "net.core.rmem_max"),
        ("net.wmem_max", "net.core.wmem_max"),
        ("net.optmem_max", "net.core.optmem_max"),
        ("net.tcp_rmem", "net.ipv4.tcp_rmem"),
        ("net.tcp_wmem", "net.ipv4.tcp_wmem"),
        ("net.somaxconn", "net.core.somaxconn"),
        ("net.netdev_max_backlog", "net.core.netdev_max_backlog"),
        ("net.tcp_max_syn_backlog", "net.ipv4.tcp_max_syn_backlog"),
        ("net.tcp_mtu_probing", "net.ipv4.tcp_mtu_probing"),
        (
            "net.tcp_slow_start_after_idle",
            "net.ipv4.tcp_slow_start_after_idle",
        ),
        ("net.tcp_tw_reuse", "net.ipv4.tcp_tw_reuse"),
        ("net.tcp_fastopen", "net.ipv4.tcp_fastopen"),
        ("net.ip_local_port_range", "net.ipv4.ip_local_port_range"),
    ] {
        hw.set(k, sysctl(key));
    }
    hw.set("net.default_if", hw.nic_iface.clone());
    hw.set("net.driver", hw.nic_driver.clone());
    hw.set(
        "net.rps_enabled",
        if hw.nic_rps_enabled { "1" } else { "0" },
    );
    hw.set("kernel.bbr", if hw.has_bbr { "1" } else { "0" });
    hw.set("cap.has_bbr", if hw.has_bbr { "1" } else { "0" });
    hw.set("cap.has_fq", if hw.has_fq { "1" } else { "0" });
    hw.set("kernel.fq", if hw.has_fq { "1" } else { "0" });
}

fn detect_workload(hw: &mut Hardware) {
    let mut names: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(rooted("/proc")) {
        for entry in rd.flatten() {
            let pid = entry.file_name().to_string_lossy().to_string();
            if !pid.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let comm = read_file(&entry.path().join("comm")).to_lowercase();
            if !comm.is_empty() {
                names.push(comm);
            }
        }
    }
    if names.is_empty() {
        if let Ok(rd) = std::fs::read_dir(rooted("/proc")) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.chars().all(|c| c.is_ascii_digit()) {
                    let comm = read_file(&entry.path().join("comm"));
                    if !comm.is_empty() {
                        names.push(comm.to_lowercase());
                    }
                }
            }
        }
    }
    hw.process_count = names.len() as i64;

    let has = |patterns: &[&str]| names.iter().any(|n| patterns.iter().any(|p| n.contains(p)));
    let mut services = Vec::new();
    if has(&[
        "mysqld",
        "mariadbd",
        "postgres",
        "redis-server",
        "mongod",
        "clickhouse",
    ]) {
        hw.workload = "database".to_string();
        services.push("database".to_string());
    }
    if has(&["libvirtd", "qemu-system", "pveproxy", "pvedaemon"]) {
        hw.workload = "virtualization-host".to_string();
        services.push("virtualization".to_string());
    }
    if has(&[
        "dockerd",
        "containerd",
        "podman",
        "kubelet",
        "k3s",
        "containerd-shim",
    ]) {
        if hw.workload.is_empty() || hw.workload == "database" {
            hw.workload = "container-host".to_string();
        }
        services.push("containers".to_string());
    }
    if has(&[
        "nginx", "apache2", "httpd", "caddy", "haproxy", "php-fpm", "varnish",
    ]) {
        if hw.workload.is_empty() {
            hw.workload = "web".to_string();
        }
        services.push("web".to_string());
    }
    if has(&["java", "srcds", "cs2", "minecraft", "paper", "forge"]) {
        if services.iter().any(|s| s == "web") || hw.workload.is_empty() {
            hw.workload = "game-server".to_string();
        }
        services.push("game".to_string());
    }
    if has(&["postfix", "dovecot", "exim"]) {
        if hw.workload.is_empty() {
            hw.workload = "mail".to_string();
        }
        services.push("mail".to_string());
    }
    if has(&["named", "unbound", "dnsmasq", "powerdns"]) {
        if hw.workload.is_empty() {
            hw.workload = "dns".to_string();
        }
        services.push("dns".to_string());
    }
    if hw.workload.is_empty() {
        hw.workload = "general-server".to_string();
    }
    hw.workload_services = services;

    let loadavg = read_file(&rooted("/proc/loadavg"));
    hw.load1 = loadavg
        .split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    hw.set("workload.class", hw.workload.clone());
    hw.set("workload.services", hw.workload_services.join(","));
    hw.set("load.1", hw.load1.to_string());
    hw.set("process.count", hw.process_count.to_string());
}

fn detect_capabilities(hw: &mut Hardware) {
    hw.proc_sys_writable = writable("/proc/sys/vm/swappiness");
    hw.sysfs_writable =
        writable("/sys/kernel/mm/transparent_hugepage/enabled") || writable("/sys/class/net");
    hw.state_writable = !hw.read_only_root;
    hw.set("cap.is_virtual", if hw.is_virtual { "1" } else { "0" });
    hw.set("cap.numa_nodes", hw.numa_nodes.to_string());
    hw.set("cap.smt", if hw.cpu_smt { "1" } else { "0" });
    hw.set(
        "os.systemd_running",
        if hw.os_systemd_running { "1" } else { "0" },
    );
    hw.set("os.supported", if hw.os_supported { "1" } else { "0" });
    hw.set(
        "cap.proc_sys_writable",
        if hw.proc_sys_writable { "1" } else { "0" },
    );
    hw.set(
        "cap.sysfs_writable",
        if hw.sysfs_writable { "1" } else { "0" },
    );
    hw.set(
        "cap.read_only_root",
        if hw.read_only_root { "1" } else { "0" },
    );
}
