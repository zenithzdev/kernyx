use kernyx::calc;
use kernyx::hardware::Hardware;

#[test]
fn storage_class_ebs() {
    let mut hw = Hardware::default();
    hw.has_ebs = true;
    assert_eq!(calc::storage_class(&hw), "ebs");
}

#[test]
fn swappiness_no_swap() {
    let mut hw = Hardware::default();
    hw.ram_mb = 7731;
    hw.swap_mb = 0;
    hw.is_virtual = true;
    let c = calc::calculate(&hw, "automatic");
    assert_eq!(c.swappiness, 10);
}
