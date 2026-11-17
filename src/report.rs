use crate::tx::*;
use anyhow::Result;

pub fn show_report() -> Result<()> {
    let txs = tx_list();
    if txs.is_empty() {
        println!("No transactions available.");
        return Ok(());
    }
    let id = txs.last().unwrap().0.clone();
    let dir = tx_dir(&id);
    let report = dir.join("report.md");
    if report.exists() {
        println!("{}", std::fs::read_to_string(&report).unwrap_or_default());
    } else {
        println!("Transaction {} has no report.", id);
    }
    Ok(())
}
