use crate::calc::Calc;
use crate::hardware::Hardware;
use crate::persist;
use crate::util::{equivalent, sysctl_proc_path, sysctl_read};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    pub id: String,
    pub category: String,
    pub kind: String,
    pub target: String,
    pub current: String,
    pub value: String,
    pub risk: String,
    pub reason: String,
    #[serde(default)]
    pub skip: bool,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub requires_reboot: bool,
    #[serde(default)]
    pub detail: String,
}

fn default_status() -> String {
    "planned".to_string()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Compare {
    Exact,
    RaiseOnly,
}

#[allow(clippy::too_many_arguments)]
fn add(
    items: &mut Vec<PlanItem>,
    status: &str,
    skip: bool,
    id: &str,
    category: &str,
    kind: &str,
    target: &str,
    current: &str,
    value: &str,
    risk: &str,
    reason: &str,
) {
    items.push(PlanItem {
        id: id.to_string(),
        category: category.to_string(),
        kind: kind.to_string(),
        target: target.to_string(),
        current: current.to_string(),
        value: value.to_string(),
        risk: risk.to_string(),
        reason: reason.to_string(),
        skip,
        status: status.to_string(),
        requires_reboot: false,
        detail: String::new(),
    });
}

fn add_advice(items: &mut Vec<PlanItem>, id: &str, category: &str, value: &str, reason: &str) {
    add(
        items, "skipped", true, id, category, "advice", id, "", value, "none", reason,
    );
}

fn add_unsupported(items: &mut Vec<PlanItem>, id: &str, category: &str, reason: &str) {
    add(
        items,
        "unsupported",
        true,
        id,
        category,
        "advice",
        id,
        "",
        "",
        "none",
        reason,
    );
}

fn last_number(value: &str) -> Option<i64> {
    value.split_whitespace().last()?.parse().ok()
}

fn values_equal(current: &str, target: &str) -> bool {
    if equivalent(current, target) {
        return true;
    }
    match (last_number(current), last_number(target)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn is_raise_only(current: &str, target: &str) -> bool {
    match (last_number(current), last_number(target)) {
        (Some(a), Some(b)) => a >= b,
        _ => false,
    }
}

fn key_present(key: &str) -> bool {
    sysctl_proc_path(key).exists() || !sysctl_read(key).is_empty()
}

fn override_text(
    overrides: &BTreeMap<String, persist::ConfigEntry>,
    key: &str,
    target: &str,
) -> Option<String> {
    overrides.get(key).and_then(|entry| {
        if equivalent(&entry.value, target) {
            None
        } else {
            Some(format!(
                "administrator override in {} (value: {})",
                entry.path.display(),
                entry.value
            ))
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn push_sysctl(
    items: &mut Vec<PlanItem>,
    overrides: &BTreeMap<String, persist::ConfigEntry>,
    category: &str,
    key: &str,
    target: &str,
    compare: Compare,
    risk: &str,
    reason: &str,
) {
    let id = format!("sysctl.{key}");
    if !key_present(key) {
        add_unsupported(
            items,
            &id,
            category,
            "kernel does not expose this parameter",
        );
        return;
    }
    if let Some(reason) = override_text(overrides, key, target) {
        add(
            items,
            "skipped",
            true,
            &id,
            category,
            "sysctl",
            key,
            &sysctl_read(key),
            target,
            risk,
            &reason,
        );
        return;
    }
    let current = sysctl_read(key);
    let unchanged = match compare {
        Compare::Exact => values_equal(&current, target),
        Compare::RaiseOnly => is_raise_only(&current, target),
    };
    if unchanged {
        add(
            items,
            "unchanged",
            true,
            &id,
            category,
            "sysctl",
            key,
            &current,
            target,
            "none",
            "already at or above the recommended value",
        );
    } else {
        add(
            items, "planned", false, &id, category, "sysctl", key, &current, target, risk, reason,
        );
    }
}

fn cpu_mask(cpus: i64) -> String {
    let cpus = cpus.clamp(1, 256) as u32;
    let mut words = Vec::new();
    let mut remaining = cpus;
    while remaining > 0 {
        let bits = remaining.min(32);
        let word = if bits == 32 {
            u32::MAX
        } else {
            (1u32 << bits) - 1
        };
        words.push(format!("{word:08x}"));
        remaining -= bits;
    }
    words.join(",")
}

pub fn build_plan(hw: &Hardware, c: &Calc, profile: &str, enable_rps: bool) -> Vec<PlanItem> {
    let mut items = Vec::new();
    let overrides = persist::scan_sysctl_overrides();

    // ---- Memory ----
    push_sysctl(
        &mut items,
        &overrides,
        "memory",
        "vm.swappiness",
        &c.swappiness.to_string(),
        Compare::Exact,
        "low",
        if hw.swap_mb == 0 {
            "No swap is active; low swappiness avoids pointless anonymous-memory reclaim."
        } else if hw.zram_active || hw.zswap_enabled {
            "Compressed swap is active; high swappiness keeps hot pages in RAM-backed zram."
        } else {
            "Balanced from RAM size, swap capacity, storage class and detected workload."
        },
    );
    push_sysctl(
        &mut items,
        &overrides,
        "memory",
        "vm.vfs_cache_pressure",
        &c.vfs_cache_pressure.to_string(),
        Compare::Exact,
        "low",
        "Controls inode/dentry cache reclaim relative to page cache.",
    );
    if c.dirty_mode == "bytes" {
        push_sysctl(
            &mut items,
            &overrides,
            "memory",
            "vm.dirty_background_bytes",
            &c.dirty_bg_bytes.to_string(),
            Compare::Exact,
            "medium",
            "Absolute background writeback threshold, sized from RAM and storage latency.",
        );
        push_sysctl(
            &mut items,
            &overrides,
            "memory",
            "vm.dirty_bytes",
            &c.dirty_bytes.to_string(),
            Compare::Exact,
            "medium",
            "Absolute dirty-page ceiling to bound writeback stalls.",
        );
    } else {
        push_sysctl(
            &mut items,
            &overrides,
            "memory",
            "vm.dirty_background_ratio",
            &c.dirty_bg_ratio.to_string(),
            Compare::Exact,
            "low",
            "Background writeback starts before dirty pages can starve the system.",
        );
        push_sysctl(
            &mut items,
            &overrides,
            "memory",
            "vm.dirty_ratio",
            &c.dirty_ratio.to_string(),
            Compare::Exact,
            "medium",
            "Caps dirty page cache relative to RAM for the detected storage latency.",
        );
    }
    push_sysctl(
        &mut items,
        &overrides,
        "memory",
        "vm.dirty_expire_centisecs",
        &c.dirty_expire.to_string(),
        Compare::Exact,
        "low",
        "Data expires to disk sooner on low-latency NVMe than on rotational media.",
    );

    push_sysctl(
        &mut items,
        &overrides,
        "memory",
        "vm.watermark_scale_factor",
        &c.watermark_scale_factor.to_string(),
        Compare::Exact,
        "low",
        "Controls kswapd aggressiveness; keeps reclaim predictable on VPS memory.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "memory",
        "vm.page-cluster",
        &c.page_cluster.to_string(),
        Compare::Exact,
        "low",
        "Disables swap readahead on SSD/NVMe to avoid unnecessary I/O.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "memory",
        "vm.compaction_proactiveness",
        &c.compaction_proactiveness.to_string(),
        Compare::Exact,
        "low",
        "Limits background compaction overhead on server workloads.",
    );
    if hw.numa_nodes > 1 {
        push_sysctl(
            &mut items,
            &overrides,
            "memory",
            "vm.zone_reclaim_mode",
            &c.zone_reclaim_mode.to_string(),
            Compare::Exact,
            "medium",
            "Avoids NUMA zone reclaim latency on multi-node systems.",
        );
    }
    if hw.overcommit_memory == 0 {
        add_advice(
            &mut items,
            "vm.overcommit_policy",
            "memory",
            "leave heuristic overcommit enabled",
            "Changing vm.overcommit_memory can turn memory pressure into allocation failures; KERNYX reports it but does not change it.",
        );
    }
    add_advice(
        &mut items,
        "vm.min_free_kbytes",
        "memory",
        &format!("current {}", sysctl_read("vm.min_free_kbytes")),
        "Kernel-managed reserve; not changed automatically because the safe value is workload- and NUMA-specific.",
    );

    // ---- Limits ----
    push_sysctl(
        &mut items,
        &overrides,
        "limits",
        "fs.file-max",
        &c.file_max.to_string(),
        Compare::RaiseOnly,
        "low",
        "System-wide file descriptor ceiling scaled to RAM.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "limits",
        "fs.nr_open",
        &c.nr_open.to_string(),
        Compare::RaiseOnly,
        "low",
        "Per-process descriptor ceiling; never lowered below the current value.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "limits",
        "kernel.pid_max",
        &c.pid_max.to_string(),
        Compare::RaiseOnly,
        "low",
        "PID space for container and service thread counts.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "limits",
        "vm.max_map_count",
        &c.max_map_count.to_string(),
        Compare::RaiseOnly,
        "low",
        "Memory-map ceiling for databases, JVMs and container runtimes.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "limits",
        "fs.inotify.max_user_watches",
        &c.inotify_watches.to_string(),
        Compare::RaiseOnly,
        "low",
        "Watch ceiling for build tools, containers and file-heavy services.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "limits",
        "fs.inotify.max_user_instances",
        &c.inotify_instances.to_string(),
        Compare::RaiseOnly,
        "low",
        "Inotify instance ceiling for multi-tenant and container hosts.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "limits",
        "fs.aio-max-nr",
        &c.aio_max_nr.to_string(),
        Compare::RaiseOnly,
        "low",
        "Asynchronous I/O request ceiling for databases and storage services.",
    );

    // ---- Network ----
    if c.enable_bbr {
        push_sysctl(
            &mut items,
            &overrides,
            "network",
            "net.ipv4.tcp_congestion_control",
            "bbr",
            Compare::Exact,
            "low",
            "BBR is available and improves throughput on lossy or variable-RTT paths.",
        );
    } else if hw.has_bbr {
        add(
            &mut items,
            "unchanged",
            true,
            "sysctl.net.ipv4.tcp_congestion_control",
            "network",
            "sysctl",
            "net.ipv4.tcp_congestion_control",
            &hw.tcp_cc,
            "bbr",
            "none",
            "BBR is available; the current congestion control is already acceptable.",
        );
    } else {
        add_unsupported(
            &mut items,
            "sysctl.net.ipv4.tcp_congestion_control",
            "network",
            "BBR is not available in this kernel; no congestion-control change is proposed.",
        );
    }
    if c.enable_fq {
        push_sysctl(
            &mut items,
            &overrides,
            "network",
            "net.core.default_qdisc",
            "fq",
            Compare::Exact,
            "low",
            "Fair queueing provides pacing for BBR and smoother egress.",
        );
    } else if hw.has_fq {
        add(
            &mut items,
            "unchanged",
            true,
            "sysctl.net.core.default_qdisc",
            "network",
            "sysctl",
            "net.core.default_qdisc",
            &hw.default_qdisc,
            "fq",
            "none",
            "Fair queueing is available; the current qdisc is already acceptable.",
        );
    }
    push_sysctl(
        &mut items,
        &overrides,
        "network",
        "net.core.rmem_max",
        &c.tcp_rmem_max.to_string(),
        Compare::RaiseOnly,
        "low",
        "Receive buffer ceiling sized from NIC speed and a 50 ms bandwidth-delay product.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "network",
        "net.core.wmem_max",
        &c.tcp_wmem_max.to_string(),
        Compare::RaiseOnly,
        "low",
        "Send buffer ceiling sized from NIC speed and RAM.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "network",
        "net.ipv4.tcp_rmem",
        &c.tcp_rmem,
        Compare::RaiseOnly,
        "low",
        "TCP autotuning range with a calculated upper bound.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "network",
        "net.ipv4.tcp_wmem",
        &c.tcp_wmem,
        Compare::RaiseOnly,
        "low",
        "TCP send autotuning range with a calculated upper bound.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "network",
        "net.core.optmem_max",
        &c.optmem_max.to_string(),
        Compare::RaiseOnly,
        "low",
        "Ancillary socket memory ceiling scaled to CPU count.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "network",
        "net.core.somaxconn",
        &c.somaxconn.to_string(),
        Compare::RaiseOnly,
        "low",
        "Listen backlog scaled to CPU count and connection-heavy workloads.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "network",
        "net.ipv4.tcp_max_syn_backlog",
        &c.syn_backlog.to_string(),
        Compare::RaiseOnly,
        "low",
        "SYN backlog for bursty connection establishment.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "network",
        "net.core.netdev_max_backlog",
        &c.netdev_backlog.to_string(),
        Compare::RaiseOnly,
        "low",
        "Per-CPU packet backlog sized from NIC speed and CPU count.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "network",
        "net.ipv4.tcp_mtu_probing",
        &c.mtu_probing.to_string(),
        Compare::RaiseOnly,
        "low",
        "Enables PMTU black-hole detection without changing firewall or routing rules.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "network",
        "net.ipv4.ip_local_port_range",
        &c.ip_local_port_range,
        Compare::Exact,
        "low",
        "Widens the ephemeral port range for outbound connection-heavy services.",
    );
    push_sysctl(
        &mut items,
        &overrides,
        "network",
        "net.ipv4.tcp_fastopen",
        &c.tcp_fastopen.to_string(),
        Compare::RaiseOnly,
        "low",
        "Enables TCP Fast Open for compatible clients and servers.",
    );
    if c.tcp_tw_reuse >= 0 {
        push_sysctl(
            &mut items,
            &overrides,
            "network",
            "net.ipv4.tcp_tw_reuse",
            &c.tcp_tw_reuse.to_string(),
            Compare::RaiseOnly,
            "low",
            "Reuses TIME_WAIT sockets for outbound connections on connection-heavy hosts.",
        );
    }
    if c.tcp_slow_start_after_idle >= 0 {
        push_sysctl(
            &mut items,
            &overrides,
            "network",
            "net.ipv4.tcp_slow_start_after_idle",
            &c.tcp_slow_start_after_idle.to_string(),
            Compare::Exact,
            "medium",
            "Performance profile keeps congestion windows warm across idle periods.",
        );
    }
    if c.conntrack_max > 0 {
        push_sysctl(
            &mut items,
            &overrides,
            "network",
            "net.netfilter.nf_conntrack_max",
            &c.conntrack_max.to_string(),
            Compare::RaiseOnly,
            "medium",
            "Connection-tracking table sized from RAM and current usage; never lowered.",
        );
    }

    // ---- CPU / scheduler ----
    if c.sched_autogroup >= 0 {
        push_sysctl(
            &mut items,
            &overrides,
            "cpu",
            "kernel.sched_autogroup_enabled",
            &c.sched_autogroup.to_string(),
            Compare::Exact,
            "low",
            "Server workloads usually manage CPU shares explicitly through cgroups.",
        );
    }
    if !c.cpu_policy.is_empty() && c.enable_governor {
        add(
            &mut items,
            "planned",
            false,
            "cpu.governor",
            "cpu",
            "cpu_governor",
            "cpufreq",
            &hw.cpu_governor,
            &c.cpu_policy,
            "low",
            "Bare-metal cpufreq policy selected from the available governors.",
        );
    } else {
        let governor_value = if hw.has_cpufreq {
            format!("current {}", hw.cpu_governor)
        } else {
            "no cpufreq interface".to_string()
        };
        add_advice(
            &mut items,
            "cpu.governor",
            "cpu",
            &governor_value,
            if hw.is_virtual {
                "CPU frequency is host-managed on this virtual machine; KERNYX does not force a governor."
            } else if !hw.has_cpufreq {
                "Firmware or the hypervisor does not expose cpufreq on this system."
            } else {
                "The current governor is already appropriate for this profile."
            },
        );
    }
    if !c.cpu_epp.is_empty() && hw.cpu_epp != c.cpu_epp {
        add(
            &mut items,
            "planned",
            false,
            "cpu.epp",
            "cpu",
            "cpu_epp",
            "cpufreq",
            &hw.cpu_epp,
            &c.cpu_epp,
            "low",
            "Energy-performance preference follows the selected profile.",
        );
    } else if !c.cpu_epp.is_empty() {
        add(
            &mut items,
            "unchanged",
            true,
            "cpu.epp",
            "cpu",
            "cpu_epp",
            "cpufreq",
            &hw.cpu_epp,
            &c.cpu_epp,
            "none",
            "energy-performance preference is already appropriate",
        );
    }

    // ---- Storage I/O ----
    if c.enable_scheduler {
        add(
            &mut items,
            "planned",
            false,
            "io.scheduler",
            "io",
            "io_scheduler",
            &hw.storage_device,
            &hw.storage_scheduler,
            &c.io_scheduler,
            "low",
            "Scheduler selected for the detected storage class and available kernel schedulers.",
        );
    } else {
        add_advice(
            &mut items,
            "io.scheduler",
            "io",
            &format!(
                "{} on {}",
                if hw.storage_scheduler.is_empty() {
                    "unknown"
                } else {
                    &hw.storage_scheduler
                },
                if hw.storage_device.is_empty() {
                    "unknown device"
                } else {
                    &hw.storage_device
                }
            ),
            if hw.is_virtual || hw.os_pve {
                "Virtual block device detected; the I/O scheduler is managed by the host."
            } else {
                "The current scheduler is already appropriate for the detected storage."
            },
        );
    }
    if c.read_ahead_kb > 0 {
        let target = format!("/sys/block/{}/queue/read_ahead_kb", hw.storage_device);
        if hw.storage_device.is_empty() {
            add_unsupported(
                &mut items,
                "io.read_ahead",
                "io",
                "root block device could not be resolved",
            );
        } else if hw.storage_read_ahead_kb >= c.read_ahead_kb {
            add(
                &mut items,
                "unchanged",
                true,
                "io.read_ahead",
                "io",
                "sysfs",
                &target,
                &hw.storage_read_ahead_kb.to_string(),
                &c.read_ahead_kb.to_string(),
                "none",
                "read-ahead is already at or above the recommended value",
            );
        } else {
            add(
                &mut items,
                "planned",
                false,
                "io.read_ahead",
                "io",
                "sysfs",
                &target,
                &hw.storage_read_ahead_kb.to_string(),
                &c.read_ahead_kb.to_string(),
                "low",
                "Sequential read-ahead sized for the detected device class; never lowered.",
            );
        }
    }
    add_advice(
        &mut items,
        "io.mount_options",
        "io",
        &format!(
            "{} on {}",
            hw.storage_fs,
            if hw.storage_mount_opts.is_empty() {
                "unknown options"
            } else {
                &hw.storage_mount_opts
            }
        ),
        "Filesystem and fstab changes are reported only; KERNYX never rewrites mount options automatically.",
    );

    // ---- Transparent huge pages ----
    if c.enable_thp {
        add(
            &mut items,
            "planned",
            false,
            "thp.mode",
            "thp",
            "sysfs_choice",
            "/sys/kernel/mm/transparent_hugepage/enabled",
            &hw.thp_mode,
            &c.thp_mode,
            "low",
            "THP is currently forced on; madvise lets applications opt in without broad memory bloat.",
        );
    } else {
        add_advice(
            &mut items,
            "thp.mode",
            "thp",
            &format!("current {}", hw.thp_mode),
            if hw.thp_mode == "never" {
                "THP is explicitly disabled; KERNYX preserves that administrator choice."
            } else {
                "THP is already in a server-appropriate mode."
            },
        );
    }

    // ---- NUMA ----
    if c.enable_numa {
        push_sysctl(
            &mut items,
            &overrides,
            "numa",
            "kernel.numa_balancing",
            &c.numa_balancing.to_string(),
            Compare::Exact,
            "low",
            "Single NUMA node detected; disabling automatic NUMA balancing removes useless scanning overhead.",
        );
    } else if hw.numa_nodes > 1 {
        add_advice(
            &mut items,
            "numa.topology",
            "numa",
            &format!("{} NUMA nodes", hw.numa_nodes),
            "Multi-node NUMA detected. KERNYX does not pin IRQs or processes automatically; use numactl or service-level CPU affinity.",
        );
    } else {
        add_advice(
            &mut items,
            "numa.topology",
            "numa",
            "single NUMA node",
            "No NUMA affinity changes are needed.",
        );
    }

    // ---- IRQ / packet distribution ----
    if hw.nic_rx_queues > 0 {
        if enable_rps && !hw.is_virtual && hw.nic_iface != "lo" {
            let mask = cpu_mask(hw.cpu_effective);
            let current = if hw.nic_rps_cpus.is_empty() {
                "0".to_string()
            } else {
                hw.nic_rps_cpus.clone()
            };
            if current == mask {
                add(
                    &mut items,
                    "unchanged",
                    true,
                    "irq.rps",
                    "irq",
                    "rps",
                    &hw.nic_iface,
                    &current,
                    &mask,
                    "none",
                    "RPS/XPS already targets all online CPUs",
                );
            } else {
                add(
                    &mut items,
                    "planned",
                    false,
                    "irq.rps",
                    "irq",
                    "rps",
                    &hw.nic_iface,
                    &current,
                    &mask,
                    "medium",
                    "Opt-in RPS/XPS distribution spreads NIC receive and transmit processing across CPUs.",
                );
            }
        } else if hw.is_virtual {
            add_advice(
                &mut items,
                "irq.rps",
                "irq",
                &format!("{} rx / {} tx queues", hw.nic_rx_queues, hw.nic_tx_queues),
                "Virtual NIC detected; packet steering is host-managed. Use --enable-rps only on bare metal.",
            );
        } else {
            add_advice(
                &mut items,
                "irq.rps",
                "irq",
                &format!("{} rx / {} tx queues", hw.nic_rx_queues, hw.nic_tx_queues),
                "RPS/XPS is not enabled by default. Use --enable-rps to opt in.",
            );
        }
    } else {
        add_advice(
            &mut items,
            "irq.rps",
            "irq",
            "no multi-queue NIC detected",
            "No RPS/XPS tuning is appropriate.",
        );
    }
    add_advice(
        &mut items,
        "irq.affinity",
        "irq",
        "IRQ affinity left unchanged",
        "KERNYX never rewrites IRQ affinity automatically on cloud or unknown platforms.",
    );

    // ---- Swap ----
    if hw.swap_mb == 0 {
        add_advice(
            &mut items,
            "swap.recommend",
            "swap",
            &format!("recommended {} MiB", crate::swap::recommended_mb(hw)),
            "No swap is configured. Run `kernyx optimize` interactively or use `--swap 4G` to create one safely.",
        );
    } else {
        add_advice(
            &mut items,
            "swap.active",
            "swap",
            &format!("{} MiB active", hw.swap_mb),
            "Existing swap is preserved; KERNYX never deletes swap automatically.",
        );
    }
    if hw.zram_active {
        add_advice(
            &mut items,
            "swap.zram",
            "swap",
            "zram active",
            "Compressed RAM swap is active; disk swap is not required by default.",
        );
    }
    if hw.zswap_enabled {
        add_advice(
            &mut items,
            "swap.zswap",
            "swap",
            "zswap enabled",
            "Zswap is active; swappiness is raised to keep compressed pages in RAM.",
        );
    }

    // ---- Kernel boot / safety notes ----
    if hw.kernel_cmdline.contains("mitigations=off") {
        add_advice(
            &mut items,
            "kernel.mitigations",
            "kernel",
            "mitigations=off present",
            "Security mitigations are disabled in the kernel command line. KERNYX does not change boot parameters.",
        );
    } else {
        add_advice(
            &mut items,
            "kernel.cmdline",
            "kernel",
            "kernel command line preserved",
            "KERNYX never modifies bootloader or kernel command-line configuration.",
        );
    }
    if hw.is_virtual {
        add_advice(
            &mut items,
            "kernel.virtualization",
            "kernel",
            &format!(
                "{} guest",
                if hw.os_virt.is_empty() {
                    "virtual"
                } else {
                    &hw.os_virt
                }
            ),
            "Guest-specific tuning is limited to settings that are meaningful inside the VM.",
        );
    }
    if hw.read_only_root {
        add_advice(
            &mut items,
            "kernel.read_only_root",
            "kernel",
            "root filesystem is read-only",
            "Configuration files cannot be persisted on a read-only root; runtime tuning is also restricted.",
        );
    }
    let _ = profile;
    items
}

pub fn filter_plan(plan: &mut Vec<PlanItem>, categories: &Option<Vec<String>>) {
    if let Some(list) = categories {
        if !list.is_empty() {
            plan.retain(|item| list.iter().any(|c| c == &item.category));
        }
    }
}

pub fn counts(plan: &[PlanItem]) -> (usize, usize, usize, usize) {
    let applied = plan
        .iter()
        .filter(|i| matches!(i.status.as_str(), "applied" | "verified"))
        .count();
    let skipped = plan
        .iter()
        .filter(|i| matches!(i.status.as_str(), "skipped" | "unchanged"))
        .count();
    let unsupported = plan.iter().filter(|i| i.status == "unsupported").count();
    let failed = plan.iter().filter(|i| i.status == "failed").count();
    (applied, skipped, unsupported, failed)
}
