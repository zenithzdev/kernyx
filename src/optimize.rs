use crate::apply;
use crate::calc::{calculate, TuningProfile};
use crate::detect::detect;
use crate::hardware::Hardware;
use crate::persist;
use crate::plan::{build_plan, counts, filter_plan, PlanItem};
use crate::swap;
use crate::tx;
use crate::ui::{Mark, Ui};
use crate::util::{now_iso, run_cmd_lossy, sysctl_read};
use anyhow::Result;
use std::io::{self, IsTerminal, Write};

pub struct Options {
    pub profile: Option<String>,
    pub safe: bool,
    pub server: bool,
    pub performance: bool,
    pub non_interactive: bool,
    pub categories: Option<String>,
    pub swap: Option<String>,
    pub enable_rps: bool,
    pub dry_run: bool,
    pub yes: bool,
    pub json: bool,
    pub quiet: bool,
}

struct Selection {
    profile: TuningProfile,
    categories: Option<Vec<String>>,
}

const KNOWN_CATEGORIES: [&str; 10] = [
    "memory", "limits", "network", "cpu", "io", "thp", "numa", "irq", "swap", "kernel",
];

fn interactive(opts: &Options, ui: &Ui) -> bool {
    ui.enabled
        && !opts.non_interactive
        && !opts.yes
        && !opts.json
        && io::stdin().is_terminal()
        && io::stdout().is_terminal()
}

fn known_profile(name: &str) -> bool {
    matches!(
        name.trim().to_lowercase().as_str(),
        "safe"
            | "conservative"
            | "server"
            | "automatic"
            | "vps"
            | "dedicated"
            | "proxmox"
            | "performance"
            | "high-performance-vps"
            | "throughput"
            | "lowlatency"
            | "game-server"
            | "gaming"
            | "custom"
    )
}

fn resolve_selection(opts: &Options, ui: &Ui) -> Result<Selection> {
    let flag_count = [opts.safe, opts.server, opts.performance]
        .iter()
        .filter(|v| **v)
        .count();
    if flag_count > 1 {
        return Err(anyhow::anyhow!(
            "choose only one of --safe, --server or --performance"
        ));
    }
    if flag_count == 1 && opts.profile.is_some() {
        return Err(anyhow::anyhow!(
            "use either --profile or --safe/--server/--performance, not both"
        ));
    }

    let explicit = if opts.safe {
        Some(TuningProfile::Safe)
    } else if opts.server {
        Some(TuningProfile::Server)
    } else if opts.performance {
        Some(TuningProfile::Performance)
    } else if let Some(name) = &opts.profile {
        if !known_profile(name) {
            return Err(anyhow::anyhow!(
                "unknown profile '{}'; use safe, server, performance or custom",
                name
            ));
        }
        Some(TuningProfile::from_name(name))
    } else {
        None
    };

    if let Some(profile) = explicit {
        let categories = if profile == TuningProfile::Custom
            && opts.categories.is_none()
            && interactive(opts, ui)
        {
            prompt_categories()?
        } else {
            parse_categories(opts.categories.as_deref())?
        };
        return Ok(Selection {
            profile,
            categories,
        });
    }

    if !interactive(opts, ui) {
        return Ok(Selection {
            profile: TuningProfile::Server,
            categories: parse_categories(opts.categories.as_deref())?,
        });
    }

    println!();
    println!("Select optimization profile:");
    println!("  1) Safe         {}", TuningProfile::Safe.description());
    println!(
        "  2) Server       {} (default)",
        TuningProfile::Server.description()
    );
    println!(
        "  3) Performance  {}",
        TuningProfile::Performance.description()
    );
    println!("  4) Custom       {}", TuningProfile::Custom.description());
    print!("Choice [2]: ");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let choice = input.trim();
    let profile = match choice {
        "1" => TuningProfile::Safe,
        "3" => TuningProfile::Performance,
        "4" => TuningProfile::Custom,
        _ => TuningProfile::Server,
    };

    let categories = if profile == TuningProfile::Custom && opts.categories.is_none() {
        prompt_categories()?
    } else {
        parse_categories(opts.categories.as_deref())?
    };

    Ok(Selection {
        profile,
        categories,
    })
}

fn prompt_categories() -> Result<Option<Vec<String>>> {
    println!();
    println!("Categories: {}", KNOWN_CATEGORIES.join(", "));
    print!("Comma-separated categories [all]: ");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    parse_categories(Some(input.trim()))
}

fn parse_categories(input: Option<&str>) -> Result<Option<Vec<String>>> {
    let Some(raw) = input.map(str::trim) else {
        return Ok(None);
    };
    if raw.is_empty() || raw.eq_ignore_ascii_case("all") {
        return Ok(None);
    }
    let mut out = Vec::new();
    for part in raw.split([',', ' ', ';']) {
        let part = part.trim().to_lowercase();
        if part.is_empty() {
            continue;
        }
        if !KNOWN_CATEGORIES.contains(&part.as_str()) {
            return Err(anyhow::anyhow!(
                "unknown category '{}'; choose from: {}",
                part,
                KNOWN_CATEGORIES.join(", ")
            ));
        }
        if !out.contains(&part) {
            out.push(part);
        }
    }
    Ok(if out.is_empty() { None } else { Some(out) })
}

fn confirm(question: &str) -> bool {
    print!("{question} [y/N]: ");
    io::stdout().flush().ok();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

fn category_title(category: &str) -> &'static str {
    match category {
        "memory" => "Kernel memory",
        "limits" => "File and process limits",
        "network" => "Network and TCP",
        "cpu" => "CPU and scheduler",
        "io" => "Storage I/O",
        "thp" => "Transparent huge pages",
        "numa" => "NUMA",
        "irq" => "IRQ and packet steering",
        "swap" => "Swap",
        "kernel" => "Kernel safety",
        _ => "Other",
    }
}

fn add_swap_item(plan: &mut Vec<PlanItem>, hw: &Hardware, size_mb: u64) {
    plan.retain(|item| {
        item.id != "swap.recommend" && item.id != "swap.active" && item.id != "swap.create"
    });
    let target = swap::default_path().to_string_lossy().to_string();
    plan.push(PlanItem {
        id: "swap.create".to_string(),
        category: "swap".to_string(),
        kind: "swap".to_string(),
        target,
        current: if hw.swap_mb == 0 {
            "none".to_string()
        } else {
            format!("{} MiB existing", hw.swap_mb)
        },
        value: format!("{size_mb} MiB"),
        risk: "medium".to_string(),
        reason: format!(
            "User-approved {size_mb} MiB swap file with mode 0600, persisted in /etc/fstab."
        ),
        skip: false,
        status: "planned".to_string(),
        requires_reboot: false,
        detail: String::new(),
    });
}

fn decide_swap(
    opts: &Options,
    hw: &Hardware,
    ui: &Ui,
    categories: &Option<Vec<String>>,
    plan: &mut Vec<PlanItem>,
) -> Result<()> {
    if categories
        .as_ref()
        .is_some_and(|list| !list.iter().any(|category| category == "swap"))
    {
        return Ok(());
    }
    let recommended = swap::recommended_mb(hw);
    let sufficient = hw.swap_mb as u64 >= (recommended / 2).max(1);

    if hw.zram_active || hw.zswap_enabled {
        if opts.swap.as_deref().is_some_and(|v| v != "none") {
            ui.warn("zram/zswap is active; refusing to add a conflicting disk swap file.");
        }
        return Ok(());
    }

    if sufficient {
        if let Some(value) = opts.swap.as_deref() {
            if value != "none" {
                ui.warn(&format!(
                    "{} MiB swap is already active; no additional swap file is needed.",
                    hw.swap_mb
                ));
            }
        }
        return Ok(());
    }

    if let Some(value) = opts.swap.as_deref() {
        let value = value.trim();
        if value.is_empty() || value.eq_ignore_ascii_case("none") {
            return Ok(());
        }
        let size = swap::parse_size(value)?;
        add_swap_item(plan, hw, size);
        return Ok(());
    }

    if !interactive(opts, ui) {
        return Ok(());
    }

    println!();
    let fs = swap::fstype();
    let avail = swap::available_space_mb().unwrap_or(0);
    println!(
        "Filesystem: {} | free space: {} MiB | recommended swap: {} MiB",
        if fs.is_empty() { "unknown" } else { &fs },
        avail,
        recommended
    );
    if avail > 0 && avail < recommended + recommended / 10 + 64 {
        ui.warn(
            "Free space is tight for the recommended swap size; a smaller size may be required.",
        );
    }
    if hw.swap_mb == 0 {
        println!("No swap is currently configured. Would you like KERNYX to create swap?");
    } else {
        println!(
            "Only {} MiB of swap is configured (recommended: {} MiB).",
            hw.swap_mb, recommended
        );
        println!("Would you like KERNYX to add a separate swap file?");
    }
    let recommended_choice = if recommended <= 2048 {
        1
    } else if recommended <= 4096 {
        2
    } else {
        3
    };
    let sizes = [2048u64, 4096, 8192];
    for (i, size) in sizes.iter().enumerate() {
        let marker = if i + 1 == recommended_choice {
            " (recommended)"
        } else {
            ""
        };
        println!("  {}) {} GiB{}", i + 1, size / 1024, marker);
    }
    println!("  4) Custom size");
    println!("  5) Skip (default)");
    print!("Choice [5]: ");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let choice = input.trim();
    let size = match choice {
        "1" => Some(sizes[0]),
        "2" => Some(sizes[1]),
        "3" => Some(sizes[2]),
        "4" => {
            print!("Size in MiB (128-65536): ");
            io::stdout().flush().ok();
            let mut custom = String::new();
            io::stdin().read_line(&mut custom).ok();
            match swap::parse_size(custom.trim()) {
                Ok(v) => Some(v),
                Err(e) => {
                    ui.warn(&format!("{e}; swap will be skipped."));
                    None
                }
            }
        }
        _ => None,
    };
    if let Some(size) = size {
        add_swap_item(plan, hw, size);
    }
    Ok(())
}

fn print_plan(ui: &Ui, plan: &[PlanItem], detailed: bool) {
    let order = [
        "memory", "limits", "network", "cpu", "io", "thp", "numa", "irq", "swap", "kernel",
    ];
    for category in order {
        let entries: Vec<&PlanItem> = plan
            .iter()
            .filter(|item| item.category == category && item.status == "planned")
            .collect();
        if entries.is_empty() {
            continue;
        }
        ui.section(category_title(category));
        for item in entries {
            ui.step(
                Mark::Active,
                &item.id,
                &format!("{} -> {}", display_value(&item.current), item.value),
            );
            ui.note(&format!("reason: {}", item.reason));
            ui.note(&format!("risk: {}", item.risk));
        }
    }

    if detailed {
        let unchanged: Vec<&PlanItem> = plan
            .iter()
            .filter(|item| item.status == "unchanged")
            .collect();
        if !unchanged.is_empty() {
            ui.section("Already optimal");
            for item in unchanged {
                ui.step(
                    Mark::Ok,
                    &item.id,
                    &format!("current: {}", display_value(&item.current)),
                );
            }
        }
    }

    let skipped: Vec<&PlanItem> = plan
        .iter()
        .filter(|item| matches!(item.status.as_str(), "skipped" | "unsupported"))
        .collect();
    if !skipped.is_empty() {
        ui.section(if detailed {
            "Skipped or unsupported"
        } else {
            "Notes and skipped items"
        });
        for item in skipped {
            let mark = if item.status == "unsupported" {
                Mark::Warn
            } else {
                Mark::Skip
            };
            ui.step(mark, &item.id, &item.reason);
        }
    }
}

fn display_value(value: &str) -> String {
    if value.is_empty() {
        "n/a".to_string()
    } else {
        value.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

fn apply_plan(
    opts: &Options,
    hw: &Hardware,
    profile: TuningProfile,
    ui: &mut Ui,
    plan: &mut [PlanItem],
) -> Result<(String, Vec<persist::FileChange>, usize, usize)> {
    let planned = plan.iter().filter(|i| i.status == "planned").count();
    let legacy = !persist::legacy_managed().is_empty();
    if planned == 0 && !legacy {
        let (_, skipped, unsupported, failed) = counts(plan);
        if opts.json {
            print_json(hw, profile, plan, false, "", &[])?;
        } else if !opts.quiet {
            ui.summary(0, skipped, unsupported, failed);
            println!("No applicable changes.");
        }
        return Ok((String::new(), Vec::new(), 0, failed));
    }

    let tx_id = tx::tx_begin(profile.name(), hw, plan)?;
    ui.section("Optimization progress");
    let total = planned;
    let mut done = 0usize;
    let mut failures = 0usize;
    let order = [
        "memory", "limits", "network", "cpu", "io", "thp", "numa", "irq", "swap", "kernel",
    ];

    for category in order {
        let indices: Vec<usize> = plan
            .iter()
            .enumerate()
            .filter(|(_, item)| item.category == category && item.status == "planned")
            .map(|(idx, _)| idx)
            .collect();
        if indices.is_empty() {
            continue;
        }
        for idx in indices {
            let item = plan[idx].clone();
            ui.progress(
                done,
                total,
                &format!("{}: {}", category_title(category), item.id),
            );
            match apply::apply_item(&item) {
                Ok(()) => {
                    plan[idx].status = "applied".to_string();
                }
                Err(e) => {
                    plan[idx].status = "failed".to_string();
                    plan[idx].detail = e.to_string();
                    failures += 1;
                    ui.error(&format!("{}: {}", item.id, e));
                }
            }
            done += 1;
        }
        ui.clear_progress();
        let applied = plan
            .iter()
            .filter(|i| {
                i.category == category && matches!(i.status.as_str(), "applied" | "verified")
            })
            .count();
        let failed = plan
            .iter()
            .filter(|i| i.category == category && i.status == "failed")
            .count();
        let detail = if failed == 0 {
            format!("{applied} applied")
        } else {
            format!("{applied} applied, {failed} failed")
        };
        ui.step(
            if failed == 0 { Mark::Ok } else { Mark::Warn },
            category_title(category),
            &detail,
        );
    }
    ui.clear_progress();

    ui.section("Persistence");
    let files = match persist::apply_persistence(plan) {
        Ok(files) => {
            if files.is_empty() {
                ui.step(Mark::Ok, "Persistence", "already up to date");
            } else {
                ui.step(
                    Mark::Ok,
                    "Persistence",
                    &format!("{} managed file(s) updated", files.len()),
                );
            }
            files
        }
        Err(e) => {
            ui.error(&format!("persistence failed: {e}"));
            failures += 1;
            Vec::new()
        }
    };

    ui.section("Verification");
    let (verified, verify_failed) = apply::verify_plan(plan);
    if verify_failed == 0 {
        ui.step(
            Mark::Ok,
            "Runtime verification",
            &format!("{verified} change(s) confirmed"),
        );
    } else {
        failures += verify_failed;
        ui.step(
            Mark::Warn,
            "Runtime verification",
            &format!("{verified} confirmed, {verify_failed} failed"),
        );
    }

    let (applied, skipped, unsupported, failed) = counts(plan);
    tx::tx_set_status(
        if failures == 0 {
            "committed"
        } else {
            "partial"
        },
        Some(&format!(
            "applied={applied} skipped={skipped} unsupported={unsupported} failed={failed}"
        )),
    );

    write_report(&tx_id, hw, profile, plan, &files)?;
    if !opts.json {
        print_summary(ui, plan, &files);
    }
    if opts.json {
        print_json(hw, profile, plan, false, &tx_id, &files)?;
    }
    tx::tx_end_cleanup_dryrun();
    Ok((tx_id, files, verified, failures))
}

fn print_summary(ui: &Ui, plan: &[PlanItem], files: &[persist::FileChange]) {
    let (applied, skipped, unsupported, failed) = counts(plan);
    ui.summary(applied, skipped, unsupported, failed);

    let changes: Vec<&PlanItem> = plan
        .iter()
        .filter(|i| matches!(i.status.as_str(), "applied" | "verified"))
        .collect();
    if !changes.is_empty() {
        ui.section("Applied changes");
        for item in &changes {
            ui.step(
                Mark::Ok,
                &item.id,
                &format!("{} -> {}", display_value(&item.current), item.value),
            );
        }
    }
    if !files.is_empty() {
        ui.section("Configuration changes");
        for file in files {
            let mark = if file.action == "removed" {
                Mark::Skip
            } else {
                Mark::Ok
            };
            ui.step(mark, file.action, &file.path);
        }
    }
    if !changes.is_empty() {
        ui.section("Changes requiring reboot");
        ui.step(
            Mark::Ok,
            "none",
            "all applied settings are active now and persisted without bootloader changes",
        );
    }
    let skipped_items: Vec<&PlanItem> = plan
        .iter()
        .filter(|i| matches!(i.status.as_str(), "skipped" | "unsupported"))
        .collect();
    if !skipped_items.is_empty() {
        ui.section("Changes that were skipped");
        for item in skipped_items {
            let mark = if item.status == "unsupported" {
                Mark::Warn
            } else {
                Mark::Skip
            };
            ui.step(mark, &item.id, &item.reason);
        }
    }
}

fn write_report(
    tx_id: &str,
    hw: &Hardware,
    profile: TuningProfile,
    plan: &[PlanItem],
    files: &[persist::FileChange],
) -> Result<()> {
    let dir = tx::tx_dir(tx_id);
    std::fs::create_dir_all(&dir).ok();
    let mut report = String::new();
    report.push_str("# KERNYX Optimization Report\n\n");
    report.push_str(&format!("- Transaction: `{}`\n", tx_id));
    report.push_str(&format!("- Created: {}\n", now_iso()));
    report.push_str(&format!("- Profile: {}\n", profile.label()));
    report.push_str(&format!("- Kernel: {}\n", hw.os_kernel));
    report.push_str(&format!("- CPU: {} ({})\n", hw.cpu_model, hw.cpu_label()));
    report.push_str(&format!("- RAM: {:.1} GiB\n", hw.ram_gib()));
    report.push_str(&format!(
        "- Storage: {} ({})\n",
        hw.storage_class, hw.storage_fs
    ));
    report.push_str(&format!(
        "- Network: {} ({})\n",
        hw.nic_iface, hw.nic_driver
    ));
    report.push_str(&format!("- Workload: {}\n", hw.workload));
    let (applied, skipped, unsupported, failed) = counts(plan);
    report.push_str("\n## Summary\n\n");
    report.push_str(&format!(
        "| Applied | Skipped | Unsupported | Failed |\n|---|---|---|---|\n| {} | {} | {} | {} |\n",
        applied, skipped, unsupported, failed
    ));
    report.push_str("\n## Changes\n\n");
    report.push_str("| Status | Category | Parameter | Before | After | Reason |\n");
    report.push_str("|---|---|---|---|---|---|\n");
    for item in plan {
        report.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            item.status,
            item.category,
            item.id,
            item.current.replace('|', "/"),
            item.value.replace('|', "/"),
            item.reason.replace('|', "/")
        ));
    }
    if !files.is_empty() {
        report.push_str("\n## Persistence\n\n");
        for file in files {
            report.push_str(&format!("- {}: `{}`\n", file.action, file.path));
        }
    }
    report.push_str("\n## Rollback\n\n");
    report.push_str(&format!(
        "```bash\nsudo kernyx optimize rollback --tx {tx_id}\n```\n"
    ));
    std::fs::write(dir.join("report.md"), report).ok();
    Ok(())
}

fn print_json(
    hw: &Hardware,
    profile: TuningProfile,
    plan: &[PlanItem],
    dry_run: bool,
    tx_id: &str,
    files: &[persist::FileChange],
) -> Result<()> {
    let (applied, skipped, unsupported, failed) = counts(plan);
    let planned = plan.iter().filter(|item| item.status == "planned").count();
    let value = serde_json::json!({
        "dry_run": dry_run,
        "transaction": if tx_id.is_empty() { serde_json::Value::Null } else { serde_json::json!(tx_id) },
        "profile": profile.name(),
        "hardware": hw,
        "counts": {
            "applied": applied,
            "planned": planned,
            "skipped": skipped,
            "unsupported": unsupported,
            "failed": failed,
        },
        "files": files.iter().map(|file| &file.path).collect::<Vec<_>>(),
        "file_changes": files,
        "items": plan,
    });
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

pub fn run(opts: Options) -> Result<()> {
    let hw = detect();
    let mut ui = Ui::new(opts.quiet, opts.json, opts.non_interactive);
    let swap_label = swap::summary(&hw).join("; ");
    ui.system(&hw, &swap_label);

    let selection = resolve_selection(&opts, &ui)?;
    let profile = selection.profile;
    ui.profile_line(profile.label(), profile.description());

    let calc = calculate(&hw, profile.name());
    let mut plan = build_plan(&hw, &calc, profile.name(), opts.enable_rps);
    filter_plan(&mut plan, &selection.categories);
    decide_swap(&opts, &hw, &ui, &selection.categories, &mut plan)?;

    if opts.dry_run {
        ui.section("Dry run - complete optimization plan");
        print_plan(&ui, &plan, true);
        let planned = plan.iter().filter(|i| i.status == "planned").count();
        let skipped = plan
            .iter()
            .filter(|i| matches!(i.status.as_str(), "skipped" | "unchanged"))
            .count();
        let unsupported = plan.iter().filter(|i| i.status == "unsupported").count();
        if !opts.json && !opts.quiet {
            ui.section("Dry run summary");
            ui.step(Mark::Active, "planned", &format!("{planned} change(s)"));
            ui.step(Mark::Skip, "skipped", &format!("{skipped} item(s)"));
            ui.step(Mark::Warn, "unsupported", &format!("{unsupported} item(s)"));
            ui.note("No runtime, file or persistent state was changed.");
        } else if !opts.json {
            println!("planned={planned} skipped={skipped} unsupported={unsupported} dry_run=1");
        }
        if opts.json {
            print_json(&hw, profile, &plan, true, "", &[])?;
        }
        return Ok(());
    }

    apply::preflight(&hw)?;

    let has_pending =
        plan.iter().any(|item| item.status == "planned") || !persist::legacy_managed().is_empty();
    if has_pending {
        ui.section("Proposed changes");
        print_plan(&ui, &plan, false);
        if interactive(&opts, &ui) && !confirm("Apply these changes?") {
            ui.note("No changes applied.");
            return Ok(());
        }
    }

    let (tx_id, files, verified, failures) = apply_plan(&opts, &hw, profile, &mut ui, &mut plan)?;
    if opts.quiet && !opts.json {
        let (applied, skipped, unsupported, failed) = counts(&plan);
        println!(
            "applied={} skipped={} unsupported={} failed={} verified={}",
            applied, skipped, unsupported, failed, verified
        );
    }
    if !tx_id.is_empty() && !opts.json && !opts.quiet {
        println!();
        println!("Transaction: {tx_id}");
        println!("Rollback: sudo kernyx optimize rollback --tx {tx_id}");
        if !files.is_empty() {
            println!("Managed configuration: {}", persist::SYSCTL_PATH);
        }
    }
    if failures > 0 {
        return Err(anyhow::anyhow!(
            "optimization completed with {} failure(s); see kernyx report",
            failures
        ));
    }
    Ok(())
}

pub fn status(json: bool) -> Result<()> {
    let hw = detect();
    let managed = persist::managed_keys();
    let dropin = crate::util::rooted(persist::SYSCTL_PATH);
    let legacy = persist::legacy_managed();
    let mut drift = Vec::new();
    for (key, persisted) in &managed {
        let runtime = sysctl_read(key);
        if !crate::util::equivalent(&runtime, persisted) {
            drift.push((key.clone(), runtime, persisted.clone()));
        }
    }
    let txs = tx::tx_list();
    let last = txs.last();
    let swap_lines = swap::summary(&hw);

    if json {
        let value = serde_json::json!({
            "hardware": hw,
            "managed_file": dropin.to_string_lossy(),
            "managed_present": dropin.exists(),
            "managed_keys": managed,
            "legacy_present": !legacy.is_empty(),
            "legacy_files": legacy.iter().map(|(path, _)| path.to_string_lossy()).collect::<Vec<_>>(),
            "runtime_drift": drift,
            "swap": swap_lines,
            "last_transaction": last.map(|t| serde_json::json!({
                "id": t.0,
                "created": t.1,
                "profile": t.2,
                "status": t.3,
            })),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }

    println!("KERNYX Optimizer Status");
    println!();
    println!("System:    {} ({})", hw.os_pretty, hw.os_kernel);
    println!("CPU:       {} ({})", hw.cpu_model, hw.cpu_label());
    println!("Memory:    {:.1} GiB", hw.ram_gib());
    println!(
        "Storage:   {} ({}, {})",
        hw.storage_class, hw.storage_device, hw.storage_fs
    );
    println!("Network:   {} ({})", hw.nic_iface, hw.nic_driver);
    println!("Workload:  {}", hw.workload);
    println!("THP:       {}", hw.thp_mode);
    println!(
        "Scheduler: {} on {}",
        if hw.storage_scheduler.is_empty() {
            "unknown"
        } else {
            &hw.storage_scheduler
        },
        if hw.storage_device.is_empty() {
            "unknown"
        } else {
            &hw.storage_device
        }
    );
    println!(
        "CPU gov:   {}",
        if hw.has_cpufreq {
            &hw.cpu_governor
        } else {
            "not exposed (virtualized or firmware-managed)"
        }
    );
    println!();
    println!(
        "Managed:   {} ({})",
        dropin.display(),
        if dropin.exists() { "present" } else { "absent" }
    );
    println!("Keys:      {}", managed.len());
    for (path, _) in &legacy {
        println!(
            "Legacy:    {} (managed drop-in); run `kernyx optimize` to migrate it",
            path.display()
        );
    }
    for (key, runtime, persisted) in &drift {
        println!("Drift:     {key} runtime={runtime} persisted={persisted}");
    }
    println!();
    println!("Swap:      {}", swap_lines.join("; "));
    if let Some((id, created, profile, status)) = last {
        println!();
        println!("Last transaction: {id} ({created}, profile={profile}, status={status})");
    } else {
        println!();
        println!("Last transaction: none");
    }
    Ok(())
}

pub fn rollback(tx_id: Option<String>) -> Result<()> {
    if tx::is_dry_run() {
        return Err(anyhow::anyhow!(
            "--dry-run cannot be combined with rollback; remove --dry-run to roll back"
        ));
    }
    if run_cmd_lossy("id", &["-u"]) != "0" {
        return Err(anyhow::anyhow!(
            "Rolling back changes requires root. Re-run with sudo."
        ));
    }
    let id = tx_id.or_else(|| tx::latest_rollbackable().map(|t| t.0));
    let Some(id) = id else {
        return Err(anyhow::anyhow!("No transaction available."));
    };
    println!("Rolling back KERNYX transaction {id}...");
    apply::transaction_rollback(&id, "manual optimize rollback")?;
    Ok(())
}
