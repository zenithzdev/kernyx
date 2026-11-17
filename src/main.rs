mod apply;
mod backup;
mod benchmark;
mod calc;
mod cli;
mod config;
mod detect;
mod doctor;
mod fetch;
mod hardware;
mod inspect;
mod monitor;
mod plan;
mod report;
mod tx;
mod util;

use crate::cli::{Cli, Commands, ConfigAction};
use crate::detect::detect;
use crate::plan::build_plan;
use anyhow::Result;
use clap::Parser;

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else {
            String::new()
        };
        if msg.contains("Broken pipe") {
            std::process::exit(0);
        }
        eprintln!("{}", info);
    }));
    let args = Cli::parse();
    if args.dry_run {
        std::env::set_var("KERNYX_DRY_RUN", "1");
    }
    if args.yes {
        std::env::set_var("KERNYX_YES", "1");
    }
    // logging simple
    if !args.quiet {
        println!("KERNYX 2.0.0");
    }
    let profile = args
        .profile
        .clone()
        .unwrap_or_else(|| "automatic".to_string());

    let res = run(args, profile);
    if let Err(e) = res {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn run(args: Cli, profile: String) -> Result<()> {
    match args.command {
        Some(Commands::Fetch) => fetch::run(),
        Some(Commands::Monitor) => monitor::run(),
        None => {
            let hw = detect();
            println!(
                "Host: {} | {} | {} | {} CPUs | {} MiB RAM",
                hw.os_pretty, hw.os_kernel, hw.os_cloud, hw.cpu_effective, hw.ram_mb
            );
            println!(
                "Run 'kernyx analyze' to see proposed changes, or 'kernyx optimize' to apply."
            );
            Ok(())
        }
        Some(Commands::Detect) => {
            let hw = detect();
            if args.json {
                println!("{}", serde_json::to_string_pretty(&hw)?);
            } else {
                println!("OS: {}", hw.os_pretty);
                println!("Kernel: {}", hw.os_kernel);
                println!("CPU: {} ({} effective)", hw.cpu_model, hw.cpu_effective);
                println!("RAM: {} MiB", hw.ram_mb);
                println!("Storage: {}", hw.storage_class);
                println!("Cloud: {}", hw.os_cloud);
                println!("Virtual: {}", hw.is_virtual);
            }
            Ok(())
        }
        Some(Commands::Analyze) => {
            let hw = detect();
            let c = calc::calculate(&hw, &profile);
            let plan = build_plan(&hw, &c, &profile);
            if args.json {
                println!("{}", serde_json::to_string_pretty(&plan)?);
            } else {
                println!("Profile: {}", profile);
                println!(
                    "Proposed changes: {}",
                    plan.iter().filter(|p| !p.skip).count()
                );
                for p in &plan {
                    if p.skip {
                        println!("  [advice] {} - {}", p.id, p.value);
                    } else {
                        println!("  {} : {} -> {} ({})", p.id, p.current, p.value, p.risk);
                    }
                }
            }
            Ok(())
        }
        Some(Commands::Optimize {
            profile: p,
            dry_run,
        }) => {
            if dry_run {
                std::env::set_var("KERNYX_DRY_RUN", "1");
            }
            let profile = p.unwrap_or(profile);
            let hw = detect();
            let c = calc::calculate(&hw, &profile);
            let plan = build_plan(&hw, &c, &profile);
            apply::apply_preflight(&hw)?;
            // drift not checked for simplicity
            apply::apply_execute_plan(&hw, &profile, &plan)
        }
        Some(Commands::Health) => inspect::health(),
        Some(Commands::Process) => inspect::process(),
        Some(Commands::Network) => inspect::network(),
        Some(Commands::Storage) => inspect::storage(),
        Some(Commands::Memory) => inspect::memory(),
        Some(Commands::Cpu) => inspect::cpu(),
        Some(Commands::Services) => inspect::services(),
        Some(Commands::Security) => inspect::security(),
        Some(Commands::Benchmark { command }) => {
            if command.is_empty() {
                inspect::benchmark()
            } else {
                benchmark::run(&command)
            }
        }
        Some(Commands::Doctor) => doctor::run(&detect()),
        Some(Commands::Backup { label }) => {
            let id = backup::create(label.as_deref().unwrap_or("manual"))?;
            println!("Snapshot created: {}", id);
            Ok(())
        }
        Some(Commands::Backups { id }) => {
            if let Some(i) = id {
                println!("Snapshot: {}", i);
            } else {
                backup::list();
            }
            Ok(())
        }
        Some(Commands::Status) => {
            let txs = tx::tx_list();
            println!("Transactions: {}", txs.len());
            for t in txs.iter().rev().take(5) {
                println!("  {} {} {} {}", t.0, t.1, t.3, t.2);
            }
            // sysctl drop-in
            let p = util::rooted("/etc/sysctl.d/99-kernyx.conf");
            println!(
                "Persistence: {}",
                if p.exists() { "present" } else { "absent" }
            );
            Ok(())
        }
        Some(Commands::Report) => report::show_report(),
        Some(Commands::Rollback { tx }) => {
            let id = tx.unwrap_or_else(|| {
                tx::tx_list()
                    .last()
                    .map(|t| t.0.clone())
                    .unwrap_or_default()
            });
            if id.is_empty() {
                return Err(anyhow::anyhow!("No transaction available."));
            }
            apply::transaction_rollback(&id, "manual")
        }
        Some(Commands::History) => {
            for t in tx::tx_list() {
                println!("{}\t{}\t{}\t{}", t.0, t.1, t.2, t.3);
            }
            Ok(())
        }
        Some(Commands::Config { action }) => match action {
            ConfigAction::Show => config::show(),
            ConfigAction::Validate => config::validate(),
        },
        Some(Commands::Version) => {
            println!("kernyx 2.0.0");
            Ok(())
        }
        Some(Commands::Uninstall) => {
            println!("Remove package with: sudo apt remove kernyx (or delete /usr/bin/kernyx)");
            Ok(())
        }
        Some(Commands::Update) => {
            println!("Update via: sudo apt update && sudo apt upgrade kernyx");
            Ok(())
        }
    }
}
