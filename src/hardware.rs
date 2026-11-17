use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    pub cpu_effective: i64,
    pub cpu_physical: i64,
    pub ram_mb: i64,
    pub swap_mb: i64,
    pub storage_class: String,
    pub has_nvme: bool,
    pub has_ssd: bool,
    pub has_hdd: bool,
    pub has_virtio: bool,
    pub has_ebs: bool,
    pub zram_active: bool,
    pub max_nic_mbps: i64,
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
}
