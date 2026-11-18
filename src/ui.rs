use crate::hardware::Hardware;
use std::io::{self, IsTerminal, Write};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Ok,
    Warn,
    Fail,
    Skip,
    Active,
    Info,
}

pub struct Ui {
    pub enabled: bool,
    pub color: bool,
    pub unicode: bool,
    pub quiet: bool,
    pub json: bool,
    progress_active: bool,
}

impl Ui {
    pub fn new(quiet: bool, json: bool, plain: bool) -> Self {
        let enabled = io::stdout().is_terminal() && !quiet && !json && !plain;
        let color = enabled
            && std::env::var("NO_COLOR").is_err()
            && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(true);
        let locale = std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LC_CTYPE"))
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default()
            .to_lowercase();
        let unicode = enabled && (locale.contains("utf-8") || locale.contains("utf8"));
        Self {
            enabled,
            color,
            unicode,
            quiet,
            json,
            progress_active: false,
        }
    }

    pub fn visible(&self) -> bool {
        !self.quiet && !self.json
    }

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn mark_text(&self, mark: Mark) -> String {
        let (unicode, ascii, color) = match mark {
            Mark::Ok => ("✓", "OK", "32"),
            Mark::Warn => ("!", "!", "33"),
            Mark::Fail => ("✗", "X", "31"),
            Mark::Skip => ("–", "-", "90"),
            Mark::Active => ("→", ">", "36"),
            Mark::Info => ("•", "*", "36"),
        };
        let symbol = if self.unicode { unicode } else { ascii };
        self.paint(color, symbol)
    }

    pub fn system(&self, hw: &Hardware, swap_label: &str) {
        if !self.visible() {
            return;
        }
        if self.enabled {
            println!(
                "{}",
                self.paint(
                    "1;36",
                    &format!("KERNYX OPTIMIZER {}", env!("CARGO_PKG_VERSION"))
                )
            );
            let rule = if self.unicode {
                "─".repeat(64)
            } else {
                "-".repeat(64)
            };
            println!("{}", self.paint("90", &rule));
        } else {
            println!("KERNYX OPTIMIZER {}", env!("CARGO_PKG_VERSION"));
        }
        let cpu = format!("{} ({})", hw.cpu_model, hw.cpu_label());
        let storage = if hw.storage_device.is_empty() {
            hw.storage_class.clone()
        } else {
            format!(
                "{} ({}, {})",
                hw.storage_class, hw.storage_device, hw.storage_fs
            )
        };
        let network = if hw.nic_iface.is_empty() {
            "unknown".to_string()
        } else {
            format!(
                "{} ({}, {} Mb/s, {} rx queues)",
                hw.nic_iface,
                if hw.nic_driver.is_empty() {
                    "unknown driver"
                } else {
                    &hw.nic_driver
                },
                hw.max_nic_mbps,
                hw.nic_rx_queues
            )
        };
        let rows = [
            ("System", hw.os_pretty.clone()),
            ("Kernel", hw.os_kernel.clone()),
            ("CPU", cpu),
            ("Memory", format!("{:.1} GiB", hw.ram_gib())),
            ("Swap", swap_label.to_string()),
            ("Storage", storage),
            ("Network", network),
            ("Workload", hw.workload.clone()),
        ];
        for (label, value) in rows {
            if self.color {
                println!("{:<10} {}", self.paint("1", label), value);
            } else {
                println!("{:<10} {}", label, value);
            }
        }
    }

    pub fn profile_line(&self, profile: &str, profile_desc: &str) {
        if !self.visible() {
            return;
        }
        let separator = if self.unicode { "—" } else { "-" };
        if self.color {
            println!(
                "{:<10} {} {}",
                self.paint("1", "Profile"),
                self.paint("1;36", profile),
                self.paint("90", &format!("{separator} {profile_desc}"))
            );
        } else {
            println!(
                "{:<10} {} {} {}",
                "Profile", profile, separator, profile_desc
            );
        }
    }

    pub fn banner(&self, hw: &Hardware, profile: &str, profile_desc: &str, swap_label: &str) {
        self.system(hw, swap_label);
        self.profile_line(profile, profile_desc);
    }

    pub fn section(&self, title: &str) {
        if !self.visible() {
            return;
        }
        println!();
        if self.color {
            println!("{}", self.paint("1", title));
        } else {
            println!("{}", title);
        }
    }

    pub fn step(&self, mark: Mark, label: &str, detail: &str) {
        if !self.visible() {
            return;
        }
        let line = if detail.is_empty() {
            format!("  {} {}", self.mark_text(mark), label)
        } else {
            format!("  {} {} {}", self.mark_text(mark), label, detail)
        };
        println!("{}", line);
    }

    pub fn note(&self, text: &str) {
        if !self.visible() {
            return;
        }
        if self.color {
            println!("    {}", self.paint("90", text));
        } else {
            println!("    {}", text);
        }
    }

    pub fn warn(&self, text: &str) {
        if !self.visible() {
            return;
        }
        if self.color {
            println!(
                "  {} {}",
                self.mark_text(Mark::Warn),
                self.paint("33", text)
            );
        } else {
            println!("  ! {}", text);
        }
    }

    pub fn error(&self, text: &str) {
        if self.color {
            eprintln!(
                "  {} {}",
                self.mark_text(Mark::Fail),
                self.paint("31", text)
            );
        } else {
            eprintln!("  X {}", text);
        }
    }

    pub fn progress(&mut self, done: usize, total: usize, label: &str) {
        if !self.enabled || total == 0 {
            return;
        }
        let width = 24usize;
        let filled = (done.saturating_mul(width) / total).min(width);
        let (full, empty) = if self.unicode {
            ("█", "░")
        } else {
            ("#", "-")
        };
        let bar = format!("{}{}", full.repeat(filled), empty.repeat(width - filled));
        let pct = done.saturating_mul(100) / total;
        let separator = if self.unicode { "│" } else { "|" };
        print!(
            "\r  {} {} {:>3}% ({}/{}) {}",
            self.paint("36", &bar),
            self.paint("90", separator),
            pct,
            done,
            total,
            label
        );
        let _ = io::stdout().flush();
        self.progress_active = true;
    }

    pub fn clear_progress(&mut self) {
        if self.progress_active {
            println!();
            self.progress_active = false;
        }
    }

    pub fn summary(&self, applied: usize, skipped: usize, unsupported: usize, failed: usize) {
        if !self.visible() {
            return;
        }
        println!();
        if self.color {
            println!("{}", self.paint("1;32", "Optimization complete"));
            println!(
                "  {} {}   {} {}   {} {}   {} {}",
                self.paint("32", "Applied:"),
                applied,
                self.paint("90", "Skipped:"),
                skipped,
                self.paint("33", "Unsupported:"),
                unsupported,
                self.paint(if failed > 0 { "31" } else { "32" }, "Failed:"),
                failed
            );
        } else {
            println!("Optimization complete");
            println!(
                "  Applied: {applied}   Skipped: {skipped}   Unsupported: {unsupported}   Failed: {failed}"
            );
        }
    }
}
