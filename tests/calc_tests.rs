use kernyx::calc::{self, TuningProfile};
use kernyx::hardware::Hardware;
use kernyx::swap;

#[test]
fn storage_class_ebs() {
    let hw = Hardware {
        has_ebs: true,
        ..Default::default()
    };
    assert_eq!(calc::storage_class(&hw), "ebs");
}

#[test]
fn swappiness_no_swap() {
    let hw = Hardware {
        ram_mb: 7731,
        swap_mb: 0,
        is_virtual: true,
        ..Default::default()
    };
    let c = calc::calculate(&hw, "automatic");
    assert_eq!(c.swappiness, 10);
}

#[test]
fn legacy_profiles_map_to_new_model() {
    assert_eq!(TuningProfile::from_name("vps"), TuningProfile::Server);
    assert_eq!(TuningProfile::from_name("dedicated"), TuningProfile::Server);
    assert_eq!(TuningProfile::from_name("proxmox"), TuningProfile::Server);
    assert_eq!(TuningProfile::from_name("automatic"), TuningProfile::Server);
    assert_eq!(
        TuningProfile::from_name("game-server"),
        TuningProfile::Performance
    );
    assert_eq!(TuningProfile::from_name("safe"), TuningProfile::Safe);
    assert_eq!(TuningProfile::default(), TuningProfile::Server);
}

#[test]
fn swap_size_parsing() {
    assert_eq!(swap::parse_size("2G").unwrap(), 2048);
    assert_eq!(swap::parse_size("512M").unwrap(), 512);
    assert_eq!(swap::parse_size("1GiB").unwrap(), 1024);
    assert_eq!(swap::parse_size("4096M").unwrap(), 4096);
    assert!(swap::parse_size("4").is_err());
    assert!(swap::parse_size("65G").is_err());
    assert!(swap::parse_size("nope").is_err());
}
