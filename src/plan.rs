use crate::calc::Calc;
use crate::hardware::Hardware;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PlanItem {
    pub id: String,
    pub category: String,
    pub kind: String,
    pub target: String,
    pub current: String,
    pub value: String,
    pub risk: String,
    pub reason: String,
    pub skip: bool,
}

pub fn build_plan(hw: &Hardware, c: &Calc, profile: &str) -> Vec<PlanItem> {
    let mut items = Vec::new();
    #[allow(clippy::too_many_arguments)]
    fn add_item(
        items: &mut Vec<PlanItem>,
        id: &str,
        cat: &str,
        kind: &str,
        target: &str,
        current: &str,
        value: &str,
        risk: &str,
        reason: &str,
    ) {
        if current == value {
            return;
        }
        items.push(PlanItem {
            id: id.to_string(),
            category: cat.to_string(),
            kind: kind.to_string(),
            target: target.to_string(),
            current: current.to_string(),
            value: value.to_string(),
            risk: risk.to_string(),
            reason: reason.to_string(),
            skip: false,
        });
    }
    fn add_advice(items: &mut Vec<PlanItem>, id: &str, cat: &str, value: &str, reason: &str) {
        items.push(PlanItem {
            id: id.to_string(),
            category: cat.to_string(),
            kind: "advice".to_string(),
            target: id.to_string(),
            current: String::new(),
            value: value.to_string(),
            risk: "medium".to_string(),
            reason: reason.to_string(),
            skip: true,
        });
    }

    // CPU
    add_item(
        &mut items,
        "sysctl.kernel.sched_autogroup_enabled",
        "cpu",
        "sysctl",
        "kernel.sched_autogroup_enabled",
        &hw.get("kernel.sched_autogroup_enabled")
            .cloned()
            .unwrap_or_default(),
        "0",
        "low",
        "Server workloads benefit from explicit cgroup CPU shares.",
    );

    // Memory
    add_item(
        &mut items,
        "sysctl.vm.swappiness",
        "memory",
        "sysctl",
        "vm.swappiness",
        &hw.get("ram.swappiness").cloned().unwrap_or_default(),
        &c.swappiness.to_string(),
        "low",
        "Calculated from RAM, swap and storage class.",
    );
    add_item(
        &mut items,
        "sysctl.vm.vfs_cache_pressure",
        "memory",
        "sysctl",
        "vm.vfs_cache_pressure",
        &hw.get("ram.vfs_cache_pressure")
            .cloned()
            .unwrap_or_default(),
        &c.vfs_cache_pressure.to_string(),
        "low",
        "Balances inode/dentry cache retention.",
    );
    add_item(
        &mut items,
        "sysctl.vm.dirty_background_ratio",
        "memory",
        "sysctl",
        "vm.dirty_background_ratio",
        &hw.get("ram.dirty_background_ratio")
            .cloned()
            .unwrap_or_default(),
        &c.dirty_bg_ratio.to_string(),
        "medium",
        "Keeps background writeback below the danger threshold.",
    );
    add_item(
        &mut items,
        "sysctl.vm.dirty_ratio",
        "memory",
        "sysctl",
        "vm.dirty_ratio",
        &hw.get("ram.dirty_ratio").cloned().unwrap_or_default(),
        &c.dirty_ratio.to_string(),
        "medium",
        "Caps dirty page cache relative to RAM.",
    );
    // dirty bytes if mode bytes
    if c.dirty_mode == "bytes" {
        add_item(
            &mut items,
            "sysctl.vm.dirty_background_bytes",
            "memory",
            "sysctl",
            "vm.dirty_background_bytes",
            &hw.get("ram.dirty_background_bytes")
                .cloned()
                .unwrap_or_default(),
            &c.dirty_bg_bytes.to_string(),
            "medium",
            "Absolute dirty writeback threshold.",
        );
        add_item(
            &mut items,
            "sysctl.vm.dirty_bytes",
            "memory",
            "sysctl",
            "vm.dirty_bytes",
            &hw.get("ram.dirty_bytes").cloned().unwrap_or_default(),
            &c.dirty_bytes.to_string(),
            "medium",
            "Absolute dirty page cap.",
        );
    }
    if c.file_max
        > hw.get("ram.file_max")
            .and_then(|x| x.parse().ok())
            .unwrap_or(0)
    {
        add_item(
            &mut items,
            "sysctl.fs.file-max",
            "kernel",
            "sysctl",
            "fs.file-max",
            &hw.get("ram.file_max").cloned().unwrap_or_default(),
            &c.file_max.to_string(),
            "low",
            "File descriptor ceiling scaled to RAM.",
        );
    }
    if c.nr_open
        > hw.get("ram.nr_open")
            .and_then(|x| x.parse().ok())
            .unwrap_or(0)
    {
        add_item(
            &mut items,
            "sysctl.fs.nr_open",
            "kernel",
            "sysctl",
            "fs.nr_open",
            &hw.get("ram.nr_open").cloned().unwrap_or_default(),
            &c.nr_open.to_string(),
            "low",
            "Per-process fd ceiling.",
        );
    }
    if c.pid_max
        > hw.get("ram.pid_max")
            .and_then(|x| x.parse().ok())
            .unwrap_or(0)
    {
        add_item(
            &mut items,
            "sysctl.kernel.pid_max",
            "kernel",
            "sysctl",
            "kernel.pid_max",
            &hw.get("ram.pid_max").cloned().unwrap_or_default(),
            &c.pid_max.to_string(),
            "low",
            "PID space for large thread counts.",
        );
    }
    if c.max_map_count
        > hw.get("ram.max_map_count")
            .and_then(|x| x.parse().ok())
            .unwrap_or(0)
    {
        add_item(
            &mut items,
            "sysctl.vm.max_map_count",
            "memory",
            "sysctl",
            "vm.max_map_count",
            &hw.get("ram.max_map_count").cloned().unwrap_or_default(),
            &c.max_map_count.to_string(),
            "low",
            "Memory-map ceiling.",
        );
    }
    // Network
    if hw.get("kernel.bbr").map(|s| s == "1").unwrap_or(false) {
        let cur_cc = hw
            .get("net.tcp_congestion_control")
            .cloned()
            .unwrap_or_default();
        if cur_cc != "bbr" {
            add_item(
                &mut items,
                "sysctl.net.ipv4.tcp_congestion_control",
                "network",
                "sysctl",
                "net.ipv4.tcp_congestion_control",
                &cur_cc,
                "bbr",
                "low",
                "BBR improves throughput on lossy paths.",
            );
        }
        let cur_q = hw.get("net.default_qdisc").cloned().unwrap_or_default();
        if hw.get("cap.has_fq").map(|s| s == "1").unwrap_or(false) && cur_q != "fq" {
            add_item(
                &mut items,
                "sysctl.net.core.default_qdisc",
                "network",
                "sysctl",
                "net.core.default_qdisc",
                &cur_q,
                "fq",
                "low",
                "FQ pacing for BBR.",
            );
        }
    }
    let cur_rmem: i64 = hw
        .get("net.rmem_max")
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    if c.tcp_rmem_max > cur_rmem {
        add_item(
            &mut items,
            "sysctl.net.core.rmem_max",
            "network",
            "sysctl",
            "net.core.rmem_max",
            &hw.get("net.rmem_max").cloned().unwrap_or_default(),
            &c.tcp_rmem_max.to_string(),
            "low",
            "BDP-based receive buffer ceiling.",
        );
    }
    let cur_wmem: i64 = hw
        .get("net.wmem_max")
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    if c.tcp_wmem_max > cur_wmem {
        add_item(
            &mut items,
            "sysctl.net.core.wmem_max",
            "network",
            "sysctl",
            "net.core.wmem_max",
            &hw.get("net.wmem_max").cloned().unwrap_or_default(),
            &c.tcp_wmem_max.to_string(),
            "low",
            "BDP-based send buffer ceiling.",
        );
    }
    let cur_tcp_wmem_max: i64 = hw
        .get("net.tcp_wmem")
        .cloned()
        .unwrap_or_default()
        .split_whitespace()
        .last()
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    if c.tcp_wmem_max > cur_tcp_wmem_max {
        add_item(
            &mut items,
            "sysctl.net.ipv4.tcp_wmem",
            "network",
            "sysctl",
            "net.ipv4.tcp_wmem",
            &hw.get("net.tcp_wmem").cloned().unwrap_or_default(),
            &format!("4096 65536 {}", c.tcp_wmem_max),
            "low",
            "Autotuning range with calculated maximum.",
        );
    }
    let cur_somax: i64 = hw
        .get("net.somaxconn")
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    if c.somaxconn > cur_somax {
        add_item(
            &mut items,
            "sysctl.net.core.somaxconn",
            "network",
            "sysctl",
            "net.core.somaxconn",
            &hw.get("net.somaxconn").cloned().unwrap_or_default(),
            &c.somaxconn.to_string(),
            "low",
            "Listen backlog scaled to CPUs.",
        );
    }
    let cur_syn: i64 = hw
        .get("net.tcp_max_syn_backlog")
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    if c.syn_backlog > cur_syn {
        add_item(
            &mut items,
            "sysctl.net.ipv4.tcp_max_syn_backlog",
            "network",
            "sysctl",
            "net.ipv4.tcp_max_syn_backlog",
            &hw.get("net.tcp_max_syn_backlog")
                .cloned()
                .unwrap_or_default(),
            &c.syn_backlog.to_string(),
            "low",
            "SYN backlog for burst connections.",
        );
    }
    let cur_nd: i64 = hw
        .get("net.netdev_max_backlog")
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    if c.netdev_backlog > cur_nd {
        add_item(
            &mut items,
            "sysctl.net.core.netdev_max_backlog",
            "network",
            "sysctl",
            "net.core.netdev_max_backlog",
            &hw.get("net.netdev_max_backlog")
                .cloned()
                .unwrap_or_default(),
            &c.netdev_backlog.to_string(),
            "low",
            "Per-CPU packet backlog.",
        );
    }
    if hw
        .get("net.tcp_mtu_probing")
        .map(|s| s == "0")
        .unwrap_or(false)
    {
        add_item(
            &mut items,
            "sysctl.net.ipv4.tcp_mtu_probing",
            "network",
            "sysctl",
            "net.ipv4.tcp_mtu_probing",
            &hw.get("net.tcp_mtu_probing").cloned().unwrap_or_default(),
            "1",
            "low",
            "PMTU black-hole detection.",
        );
    }

    // recommendations
    add_advice(
        &mut items,
        "storage.mountopts",
        "storage",
        "Review mount options",
        "Mount/fstab changes are never applied automatically.",
    );
    add_advice(
        &mut items,
        "net.conntrack.hashsize",
        "network",
        "Conntrack hash size is module param",
        "Change only at module load/reboot.",
    );
    add_advice(
        &mut items,
        "nic.rps",
        "nic",
        "RPS/XPS opt-in",
        "Not applied unless --enable-rps.",
    );
    add_advice(
        &mut items,
        "swap.recommend",
        "swap",
        "Consider zram",
        "No swap detected.",
    );
    add_advice(
        &mut items,
        "report.measure",
        "verify",
        "Baselines optional",
        "Never fabricated.",
    );
    add_advice(
        &mut items,
        "report.rollback",
        "verify",
        "Rollback available",
        "Use kernyx rollback.",
    );
    add_advice(
        &mut items,
        "limits.virtual",
        "verify",
        "Virtual host limits",
        "Some controls unavailable.",
    );

    let _ = profile;
    items
}
