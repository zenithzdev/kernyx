use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Hardware {
    pub os_id: String,
    pub os_name: String,
    pub os_version: String,
    pub os_pretty: String,
    pub os_kernel: String,
    pub os_arch: String,
    pub os_pve: bool,
    pub os_cloud: String,
    pub os_virt: String,
    pub os_is_vm: bool,
    pub os_is_container: bool,
    pub os_systemd_running: bool,
    pub os_supported: bool,

    pub cpu_model: String,
    pub cpu_vendor: String,
    pub cpu_effective: i64,
    pub cpu_physical: i64,
    pub cpu_cores: i64,
    pub cpu_sockets: i64,
    pub cpu_smt: bool,
    pub numa_nodes: i64,
    pub numa_online: String,
    pub cpu_driver: String,
    pub cpu_governor: String,
    pub cpu_governors: String,
    pub cpu_epp: String,
    pub cpu_epp_available: String,
    pub has_cpufreq: bool,

    pub ram_mb: i64,
    pub mem_available_mb: i64,
    pub swap_mb: i64,
    pub swap_free_mb: i64,
    pub zram_active: bool,
    pub zswap_enabled: bool,
    pub pressure_memory: f64,
    pub pressure_cpu: f64,
    pub pressure_io: f64,

    pub storage_class: String,
    pub storage_device: String,
    pub storage_source: String,
    pub storage_fs: String,
    pub storage_mount_opts: String,
    pub storage_scheduler: String,
    pub storage_schedulers: String,
    pub storage_read_ahead_kb: i64,
    pub storage_nr_requests: i64,
    pub storage_discard: bool,
    pub has_nvme: bool,
    pub has_ssd: bool,
    pub has_hdd: bool,
    pub has_virtio: bool,
    pub has_ebs: bool,

    pub max_nic_mbps: i64,
    pub nic_driver: String,
    pub nic_iface: String,
    pub nic_rx_queues: i64,
    pub nic_tx_queues: i64,
    pub nic_rps_enabled: bool,
    pub nic_rps_cpus: String,
    pub nic_xps_cpus: String,
    pub tcp_cc: String,
    pub tcp_available_cc: String,
    pub default_qdisc: String,
    pub has_bbr: bool,
    pub has_fq: bool,
    pub conntrack: bool,
    pub conntrack_max: i64,
    pub conntrack_count: i64,

    pub thp_mode: String,
    pub thp_enabled: String,
    pub thp_defrag: String,
    pub overcommit_memory: i64,
    pub numa_balancing: i64,
    pub inotify_watches: i64,
    pub inotify_instances: i64,
    pub aio_max_nr: i64,

    pub workload: String,
    pub workload_services: Vec<String>,
    pub load1: f64,
    pub process_count: i64,
    pub kernel_cmdline: String,
    pub read_only_root: bool,
    pub proc_sys_writable: bool,
    pub sysfs_writable: bool,
    pub state_writable: bool,

    pub is_virtual: bool,
    pub profile_id: String,
    pub values: BTreeMap<String, String>,
}

impl Hardware {
    pub fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }

    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        self.values.insert(key.to_string(), value.into());
    }

    pub fn cpu_label(&self) -> String {
        let mut s = format!("{} logical", self.cpu_effective);
        if self.cpu_cores > 0 && self.cpu_cores != self.cpu_effective {
            s.push_str(&format!(" / {} cores", self.cpu_cores));
        }
        if self.cpu_sockets > 1 {
            s.push_str(&format!(" / {} sockets", self.cpu_sockets));
        }
        if self.cpu_smt {
            s.push_str(" / SMT");
        }
        s
    }

    pub fn ram_gib(&self) -> f64 {
        self.ram_mb as f64 / 1024.0
    }
}
