use crate::hardware::Hardware;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TuningProfile {
    Safe,
    #[default]
    Server,
    Performance,
    Custom,
}

impl TuningProfile {
    pub fn from_name(name: &str) -> Self {
        match name.trim().to_lowercase().as_str() {
            "safe" | "conservative" => Self::Safe,
            "performance"
            | "high-performance-vps"
            | "throughput"
            | "lowlatency"
            | "game-server"
            | "gaming" => Self::Performance,
            "custom" => Self::Custom,
            _ => Self::Server,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Server => "server",
            Self::Performance => "performance",
            Self::Custom => "custom",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Safe => "Safe",
            Self::Server => "Server",
            Self::Performance => "Performance",
            Self::Custom => "Custom",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Safe => "Conservative kernel tuning; minimal, reversible changes",
            Self::Server => "Balanced VPS/server performance and stability",
            Self::Performance => "More aggressive throughput tuning; review trade-offs",
            Self::Custom => "Choose which optimization categories to apply",
        }
    }

    pub fn is_safe(self) -> bool {
        self == Self::Safe
    }

    pub fn is_performance(self) -> bool {
        self == Self::Performance
    }
}

#[derive(Debug, Clone, Default)]
pub struct Calc {
    pub profile: String,
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
    pub inotify_instances: i64,
    pub aio_max_nr: i64,
    pub cpu_policy: String,
    pub cpu_epp: String,
    pub somaxconn: i64,
    pub syn_backlog: i64,
    pub tcp_rmem_max: i64,
    pub tcp_wmem_max: i64,
    pub tcp_rmem: String,
    pub tcp_wmem: String,
    pub optmem_max: i64,
    pub netdev_backlog: i64,
    pub mtu_probing: i64,
    pub ip_local_port_range: String,
    pub tcp_fastopen: i64,
    pub tcp_tw_reuse: i64,
    pub tcp_slow_start_after_idle: i64,
    pub conntrack_max: i64,
    pub sched_autogroup: i64,
    pub numa_balancing: i64,
    pub thp_mode: String,
    pub io_scheduler: String,
    pub read_ahead_kb: i64,
    pub enable_bbr: bool,
    pub enable_fq: bool,
    pub enable_governor: bool,
    pub enable_thp: bool,
    pub enable_scheduler: bool,
    pub enable_numa: bool,
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

fn includes(list: &str, value: &str) -> bool {
    list.split_whitespace().any(|v| v == value)
}

pub fn calculate(hw: &Hardware, profile: &str) -> Calc {
    let p = TuningProfile::from_name(profile);
    let storage = storage_class(hw);
    let ram = hw.ram_mb.max(1);
    let ram_bytes = ram.saturating_mul(1024 * 1024);
    let cpu = hw.cpu_effective.max(1);
    let workload = hw.workload.as_str();
    let is_db = workload == "database";
    let is_web = matches!(workload, "web" | "dns" | "mail");
    let is_container_host = workload == "container-host";
    let is_virt_host = workload == "virtualization-host";
    let is_game = workload == "game-server";
    let needs_many_files = is_container_host || is_virt_host || is_web || is_game;

    let mut c = Calc {
        profile: p.name().to_string(),
        storage_class: storage.clone(),
        ..Default::default()
    };

    // Memory reclaim behaviour.
    let mut swappiness = if hw.zram_active || hw.zswap_enabled {
        100
    } else if hw.swap_mb == 0 || is_db || is_virt_host {
        10
    } else if ram <= 2048 {
        60
    } else if ram <= 4096 {
        40
    } else if ram <= 8192 {
        30
    } else if ram <= 16384 {
        20
    } else {
        10
    };
    if storage == "hdd" {
        swappiness += 5;
    }
    if p.is_performance() && hw.swap_mb > 0 {
        swappiness -= 10;
    }
    if p.is_safe() {
        swappiness = swappiness.clamp(10, 40);
    }
    c.swappiness = swappiness.clamp(10, 100);

    c.vfs_cache_pressure = if p.is_safe() {
        100
    } else if ram >= 65536 {
        50
    } else if is_db || is_virt_host || ram >= 16384 {
        75
    } else if ram < 8192 || is_container_host {
        125
    } else {
        100
    }
    .clamp(10, 200);

    // Dirty page writeback. Use absolute bytes on larger hosts, ratios on small ones.
    let dirty_bytes_current: i64 = hw
        .get("ram.dirty_bytes")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let bytes_mode = hw.ram_mb > 8192 || dirty_bytes_current > 0;
    if bytes_mode {
        c.dirty_mode = "bytes".into();
        let (bg_pct, dirty_pct): (f64, f64) = match storage.as_str() {
            "hdd" => (0.5, 2.0),
            "ebs" | "virtio" => (1.0, 3.0),
            "nvme" | "ssd" => (1.0, 5.0),
            _ => (1.0, 3.0),
        };
        let (bg_pct, dirty_pct) = if is_db {
            (bg_pct.min(0.5), dirty_pct.min(2.0))
        } else if p.is_performance() {
            (bg_pct, dirty_pct.max(6.0))
        } else {
            (bg_pct, dirty_pct)
        };
        let mut bg = (ram_bytes as f64 * bg_pct / 100.0) as i64;
        let mut dirty = (ram_bytes as f64 * dirty_pct / 100.0) as i64;
        bg = ((bg / 1_048_576).max(64)) * 1_048_576;
        dirty = ((dirty / 1_048_576).max(256)) * 1_048_576;
        bg = bg.min(2 * 1024 * 1_048_576);
        dirty = dirty.min(8 * 1024 * 1_048_576);
        if dirty < bg * 2 {
            dirty = bg * 2;
        }
        c.dirty_bg_bytes = bg;
        c.dirty_bytes = dirty;
        c.dirty_bg_ratio = 0;
        c.dirty_ratio = 0;
    } else {
        c.dirty_mode = "ratio".into();
        c.dirty_bg_ratio = 5;
        c.dirty_ratio = match storage.as_str() {
            "hdd" | "ebs" | "virtio" => 10,
            _ => 15,
        };
        if is_db {
            c.dirty_ratio = 10;
        }
        if p.is_performance() {
            c.dirty_ratio = 20;
        }
    }
    c.dirty_expire = match storage.as_str() {
        "nvme" | "ssd" => 1500,
        "hdd" => 3000,
        _ => 2000,
    };

    // Descriptor and process ceilings. These are raised, never lowered.
    let mut file_max = ram.saturating_mul(128);
    if p.is_safe() {
        file_max = ram.saturating_mul(64);
    }
    c.file_max = file_max.clamp(65536, 8_388_608);
    c.file_max = ((c.file_max + 1023) / 1024) * 1024;
    c.nr_open = c.file_max.clamp(1_048_576, 8_388_608);
    c.pid_max = (cpu.saturating_mul(2048))
        .max(ram.saturating_mul(16))
        .clamp(32768, 4_194_304);
    c.max_map_count = if is_db || is_game || is_container_host || is_virt_host {
        262_144
    } else {
        ram.saturating_mul(32).clamp(65_530, 1_048_576)
    };
    c.inotify_watches = if needs_many_files {
        ram.saturating_mul(128).max(1_048_576)
    } else {
        ram.saturating_mul(64)
    }
    .clamp(524_288, 4_194_304);
    c.inotify_instances = if is_container_host || is_virt_host {
        1024
    } else if needs_many_files {
        512
    } else {
        256
    };
    c.aio_max_nr = if is_db || p.is_performance() {
        1_048_576
    } else if is_container_host || is_virt_host {
        524_288
    } else {
        262_144
    };

    // CPU frequency policy: only meaningful on bare metal with cpufreq.
    if !p.is_safe() && hw.has_cpufreq && !hw.is_virtual {
        if p.is_performance() {
            if includes(&hw.cpu_governors, "performance") {
                c.cpu_policy = "performance".into();
            } else if includes(&hw.cpu_governors, "schedutil") {
                c.cpu_policy = "schedutil".into();
            }
        } else if includes(&hw.cpu_governors, "schedutil") {
            c.cpu_policy = "schedutil".into();
        } else if includes(&hw.cpu_governors, "powersave") {
            c.cpu_policy = "powersave".into();
        }
        if !c.cpu_policy.is_empty() {
            if p.is_performance() && includes(&hw.cpu_epp_available, "performance") {
                c.cpu_epp = "performance".into();
            } else if includes(&hw.cpu_epp_available, "balance_performance") {
                c.cpu_epp = "balance_performance".into();
            }
        }
    }
    c.enable_governor = !c.cpu_policy.is_empty() && c.cpu_policy != hw.cpu_governor;

    // Network backlog and socket ceilings.
    let speed = hw.max_nic_mbps.max(100);
    let bdp = speed.saturating_mul(6_250); // ~50 ms BDP in bytes
    let buffer_cap = if speed >= 10_000 {
        32 * 1024 * 1024
    } else if speed >= 1_000 {
        8 * 1024 * 1024
    } else if speed >= 100 {
        4 * 1024 * 1024
    } else {
        2 * 1024 * 1024
    };
    let ram_cap = ram_bytes / 32;
    let mut socket_max = bdp.min(buffer_cap).min(ram_cap).max(4 * 1024 * 1024);
    if p.is_safe() {
        socket_max = socket_max.min(4 * 1024 * 1024);
    }
    c.tcp_rmem_max = socket_max;
    c.tcp_wmem_max = socket_max;
    c.tcp_rmem = format!("4096 131072 {socket_max}");
    c.tcp_wmem = format!("4096 65536 {socket_max}");
    c.optmem_max = (cpu.saturating_mul(16_384)).clamp(65_536, 262_144);

    let mut somax = (cpu.saturating_mul(1024)).max(4096);
    if is_web || is_container_host || p.is_performance() {
        somax = 65_535;
    } else if is_game {
        somax = 32_768;
    } else if is_db {
        somax = 8192;
    }
    if p.is_safe() {
        somax = (cpu.saturating_mul(256)).max(1024);
    }
    c.somaxconn = somax.clamp(1024, 65_535);
    c.syn_backlog = c.somaxconn.max(4096).clamp(1024, 65_535);

    c.netdev_backlog = if speed >= 10_000 {
        65_536
    } else if speed >= 1_000 {
        16_384
    } else {
        8_192
    }
    .max(cpu.saturating_mul(1024))
    .min(65_536);
    if p.is_safe() {
        c.netdev_backlog = c.netdev_backlog.min(8192);
    }
    c.mtu_probing = 1;
    c.ip_local_port_range = "10240 65535".to_string();
    c.tcp_fastopen = if p.is_safe() { 1 } else { 3 };
    c.tcp_tw_reuse = if is_web || is_container_host || p.is_performance() {
        1
    } else {
        -1
    };
    c.tcp_slow_start_after_idle = if p.is_performance() { 0 } else { -1 };
    c.enable_bbr = !p.is_safe() && hw.has_bbr && hw.tcp_cc != "bbr";
    c.enable_fq = !p.is_safe() && hw.has_fq && hw.default_qdisc != "fq";

    if hw.conntrack {
        let mut ct = ram.saturating_mul(128).clamp(65_536, 4_194_304);
        ct = ct.max(hw.conntrack_count.saturating_mul(2));
        c.conntrack_max = ct;
    }

    // Scheduler and NUMA.
    c.sched_autogroup = if p.is_safe() { -1 } else { 0 };
    c.numa_balancing = if p.is_safe() {
        hw.numa_balancing
    } else if hw.numa_nodes <= 1 {
        0
    } else {
        hw.numa_balancing
    };
    c.enable_numa = !p.is_safe() && hw.numa_nodes <= 1 && hw.numa_balancing != c.numa_balancing;

    // Transparent huge pages: only correct an over-aggressive "always" setting.
    c.thp_mode = if !p.is_safe() && hw.thp_mode == "always" {
        "madvise".to_string()
    } else {
        String::new()
    };
    c.enable_thp = !c.thp_mode.is_empty() && hw.thp_mode != c.thp_mode;

    // I/O scheduler. Virtual disks are host-managed; do not pretend to tune them.
    if !p.is_safe() && !hw.is_virtual && !hw.os_pve && !hw.storage_device.is_empty() {
        let available = &hw.storage_schedulers;
        c.io_scheduler = match storage.as_str() {
            "nvme" | "ebs" => {
                if includes(available, "none") {
                    "none".to_string()
                } else if includes(available, "mq-deadline") {
                    "mq-deadline".to_string()
                } else {
                    String::new()
                }
            }
            "hdd" => {
                if includes(available, "bfq") {
                    "bfq".to_string()
                } else if includes(available, "mq-deadline") {
                    "mq-deadline".to_string()
                } else {
                    String::new()
                }
            }
            _ => {
                if includes(available, "mq-deadline") {
                    "mq-deadline".to_string()
                } else if includes(available, "none") {
                    "none".to_string()
                } else {
                    String::new()
                }
            }
        };
    }
    c.enable_scheduler = !c.io_scheduler.is_empty() && c.io_scheduler != hw.storage_scheduler;

    c.read_ahead_kb = match storage.as_str() {
        "hdd" => 2048,
        "nvme" | "ebs" | "ssd" => 256,
        "virtio" => 256,
        _ => 0,
    };
    if p.is_safe() {
        c.read_ahead_kb = 0;
    }

    c
}
