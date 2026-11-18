use anyhow::Result;
use clap::Parser;
use kernyx::cli::{Cli, Commands, ConfigAction, OptimizeAction};
use kernyx::{
    backup, benchmark, calc, config, detect, doctor, fetch, inspect, monitor, optimize, persist,
    plan, report, tx, util,
};

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

    let suppress_banner = args.quiet
        || args.json
        || matches!(
            args.command,
            Some(Commands::Optimize { .. })
                | Some(Commands::Monitor)
                | Some(Commands::Fetch)
                | Some(Commands::Benchmark { .. })
        );
    if !suppress_banner {
        println!("KERNYX {}", env!("CARGO_PKG_VERSION"));
    }

    if let Err(e) = run(args) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn run(args: Cli) -> Result<()> {
    match args.command {
        Some(Commands::Fetch) => fetch::run(),
        Some(Commands::Monitor) => monitor::run(),
        None => {
            let hw = detect::detect();
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
            let hw = detect::detect();
            if args.json {
                println!("{}", serde_json::to_string_pretty(&hw)?);
            } else {
                println!("OS: {}", hw.os_pretty);
                println!("Kernel: {}", hw.os_kernel);
                println!("CPU: {} ({})", hw.cpu_model, hw.cpu_label());
                println!("RAM: {} MiB", hw.ram_mb);
                println!(
                    "Storage: {} ({}, {})",
                    hw.storage_class, hw.storage_device, hw.storage_fs
                );
                println!("Network: {} ({})", hw.nic_iface, hw.nic_driver);
                println!("Cloud: {}", hw.os_cloud);
                println!("Virtual: {}", hw.is_virtual);
                println!("Workload: {}", hw.workload);
            }
            Ok(())
        }
        Some(Commands::Analyze) => {
            let profile = args
                .profile
                .clone()
                .unwrap_or_else(|| "automatic".to_string());
            let hw = detect::detect();
            let c = calc::calculate(&hw, &profile);
            let plan = plan::build_plan(&hw, &c, &profile, false);
            if args.json {
                println!("{}", serde_json::to_string_pretty(&plan)?);
            } else {
                println!("Profile: {} ({})", c.profile, profile);
                let planned = plan.iter().filter(|p| p.status == "planned").count();
                println!("Proposed changes: {planned}");
                for p in &plan {
                    match p.status.as_str() {
                        "planned" => {
                            println!("  {} : {} -> {} ({})", p.id, p.current, p.value, p.risk)
                        }
                        "unchanged" => println!("  [ok] {} ({})", p.id, p.current),
                        "unsupported" => println!("  [unsupported] {} - {}", p.id, p.reason),
                        "skipped" => println!("  [skip] {} - {}", p.id, p.reason),
                        _ => println!("  [{}] {} - {}", p.status, p.id, p.value),
                    }
                }
                if !args.json {
                    println!();
                    println!("Load and resource analysis:");
                    optimize::print_load_analysis();
                    println!();
                    optimize::print_service_analysis();
                }
            }
            Ok(())
        }
        Some(Commands::Optimize {
            action,
            server,
            safe,
            performance,
            categories,
            swap,
            enable_rps,
        }) => match action {
            Some(OptimizeAction::Status) => optimize::status(args.json),
            Some(OptimizeAction::Rollback { tx }) => optimize::rollback(tx),
            None => optimize::run(optimize::Options {
                profile: args.profile.clone(),
                safe,
                server,
                performance,
                non_interactive: args.non_interactive,
                categories,
                swap,
                enable_rps,
                dry_run: args.dry_run,
                yes: args.yes,
                json: args.json,
                quiet: args.quiet,
            }),
        },
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
        Some(Commands::Doctor) => doctor::run(&detect::detect()),
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
            let current = util::rooted(persist::SYSCTL_PATH);
            let legacy = persist::legacy_managed();
            println!(
                "Persistence: {}",
                if current.exists() {
                    "present (managed)"
                } else if !legacy.is_empty() {
                    "legacy drop-in"
                } else {
                    "absent"
                }
            );
            println!("Run 'kernyx optimize status' for optimizer details.");
            Ok(())
        }
        Some(Commands::Report) => report::show_report(),
        Some(Commands::Rollback { tx }) => optimize::rollback(tx),
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
            println!("kernyx {}", env!("CARGO_PKG_VERSION"));
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
