use crate::hardware::Hardware;

#[derive(Debug, Clone, Default)]
pub struct Calc {
    pub storage_class: String,
    pub swappiness: i64,
    pub vfs_cache_pressure: i64,
    pub dirty_mode: String,
    pub dirty_bg_ratio: i64,
    pub dirty_ratio: i64,
    pub dirty_bg_bytes: i64,
    pub dirty_bytes: i64,
    pub dirty_expire: i64,
    pub file_max: i64,
    pub nr_open: i64,
    pub pid_max: i64,
    pub max_map_count: i64,
    pub inotify_watches: i64,
    pub cpu_policy: String,
    pub cpu_epp: String,
    pub somaxconn: i64,
    pub syn_backlog: i64,
    pub tcp_rmem_max: i64,
    pub tcp_wmem_max: i64,
    pub optmem_max: i64,
    pub netdev_backlog: i64,
    pub mtu_probing: i64,
}

pub fn storage_class(hw: &Hardware) -> String {
    if hw.has_nvme {
        "nvme".into()
    } else if hw.has_ebs {
        "ebs".into()
    } else if hw.has_ssd {
        "ssd".into()
    } else if hw.has_hdd {
        "hdd".into()
    } else if hw.has_virtio {
        "virtio".into()
    } else {
        "unknown".into()
    }
}

pub fn calculate(hw: &Hardware, profile: &str) -> Calc {
    let mut c = Calc {
        storage_class: storage_class(hw),
        ..Default::default()
    };
    let ram = hw.ram_mb;
    let cpu = hw.cpu_effective.max(1);
    let is_virtual = hw.is_virtual;
    let pve = hw.os_pve;

    // swappiness
    if hw.zram_active {
        c.swappiness = 100;
    } else if pve || hw.swap_mb == 0 {
        c.swappiness = 10;
    } else if ram <= 2048 {
        c.swappiness = 30;
    } else if ram <= 4096 {
        c.swappiness = 25;
    } else if ram <= 8192 {
        c.swappiness = 20;
    } else if ram <= 32768 {
        c.swappiness = 15;
    } else {
        c.swappiness = 10;
    }
    if c.storage_class == "hdd" && c.swappiness < 30 {
        c.swappiness += 5;
    }
    c.swappiness = c.swappiness.clamp(10, 60);

    // vfs cache pressure
    if pve {
        c.vfs_cache_pressure = 75;
    } else if ram >= 65536 {
        c.vfs_cache_pressure = 50;
    } else if ram >= 16384 {
        c.vfs_cache_pressure = 75;
    } else if ram < 8192 {
        c.vfs_cache_pressure = 125;
    } else {
        c.vfs_cache_pressure = 100;
    }
    c.vfs_cache_pressure = c.vfs_cache_pressure.clamp(10, 200);

    // dirty
    c.dirty_expire = match c.storage_class.as_str() {
        "nvme" | "ssd" => 1500,
        _ => 3000,
    };
    if ram <= 8192 {
        c.dirty_mode = "ratio".into();
        c.dirty_bg_ratio = 5;
        c.dirty_ratio = match c.storage_class.as_str() {
            "hdd" | "ebs" => 10,
            _ => 15,
        };
        if profile == "high-performance-vps" || profile == "throughput" {
            c.dirty_ratio = 20;
        }
    } else {
        c.dirty_mode = "bytes".into();
        let (bg_pct, dirty_pct, bg_max, dirty_max) = match c.storage_class.as_str() {
            "hdd" => (0.5, 2.0, 512, 2048),
            "ebs" | "virtio" => (1.0, 3.0, 512, 2048),
            "nvme" | "ssd" => (1.0, 5.0, 1024, 4096),
            _ => (1.0, 3.0, 512, 2048),
        };
        let mut bg = (ram as f64 * 1024.0 * 1024.0 * bg_pct / 100.0) as i64;
        let mut dirty = (ram as f64 * 1024.0 * 1024.0 * dirty_pct / 100.0) as i64;
        // round down to 1MB
        bg = (bg / 1048576) * 1048576;
        dirty = (dirty / 1048576) * 1048576;
        let bg_min = 64 * 1048576;
        let dirty_min = 256 * 1048576;
        if bg < bg_min {
            bg = bg_min;
        }
        if bg > bg_max * 1048576 {
            bg = bg_max * 1048576;
        }
        if dirty < dirty_min {
            dirty = dirty_min;
        }
        if dirty > dirty_max * 1048576 {
            dirty = dirty_max * 1048576;
        }
        if dirty < bg * 2 {
            dirty = bg * 2;
        }
        c.dirty_bg_bytes = bg;
        c.dirty_bytes = dirty;
        c.dirty_bg_ratio = 0;
        c.dirty_ratio = 0;
    }

    // file max
    let mut file_max = ram * 128;
    if file_max < 65536 {
        file_max = 65536;
    }
    if profile == "high-connection" && file_max < 2097152 {
        file_max = 2097152;
    }
    if file_max > 8388608 {
        file_max = 8388608;
    }
    file_max = (file_max + 1023) / 1024 * 1024;
    c.file_max = file_max;
    c.nr_open = file_max.clamp(1048576, 8388608);

    // pid max
    let mut pid_max = cpu * 2048;
    if ram * 16 > pid_max {
        pid_max = ram * 16;
    }
    pid_max = pid_max.clamp(32768, 4194304);
    c.pid_max = pid_max;

    // max map count
    c.max_map_count = match profile {
        "minecraft" | "storage" | "high-connection" | "proxmox" => 262144,
        _ => (ram * 32).max(65530),
    };
    if c.max_map_count > 4194304 {
        c.max_map_count = 4194304;
    }

    // inotify
    let mut inotify = ram * 64;
    inotify = inotify.clamp(524288, 4194304);
    c.inotify_watches = inotify;

    // cpu policy
    if is_virtual {
        c.cpu_policy = "leave".into();
        c.cpu_epp = String::new();
    } else {
        c.cpu_policy = "schedutil".into();
        c.cpu_epp = "balance_performance".into();
    }

    // somaxconn
    let mut somax = cpu * 256;
    match profile {
        "high-connection" => somax = 65535,
        "throughput" => {
            if somax < 32768 {
                somax = 32768;
            }
        }
        "lowlatency" if somax < 8192 => {
            somax = 8192;
        }
        _ => {}
    }
    c.somaxconn = somax.clamp(4096, 65535);

    // syn backlog
    let mut syn = c.somaxconn;
    if syn < 8192 {
        syn = 8192;
    }
    c.syn_backlog = syn.clamp(1024, 65535);

    // tcp buffers
    c.tcp_rmem_max = 12582912;
    c.tcp_wmem_max = 12582912;
    c.optmem_max = (cpu * 8192).clamp(32768, 131072);

    // netdev backlog
    c.netdev_backlog = (cpu * 1536).clamp(1000, 65536);
    // mtu probing
    c.mtu_probing = 1;

    let _ = profile;
    c
}
