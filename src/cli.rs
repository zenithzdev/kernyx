use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "kernyx",
    version = env!("CARGO_PKG_VERSION"),
    about = "KERNYX - Adaptive Linux Performance Optimization Engine"
)]
pub struct Cli {
    /// Emit machine-readable JSON (disables animation)
    #[arg(long, global = true)]
    pub json: bool,
    /// Only print final status lines
    #[arg(long, global = true)]
    pub quiet: bool,
    /// Show additional detection detail
    #[arg(long, global = true)]
    pub verbose: bool,
    /// Show what would change without modifying the system
    #[arg(long, global = true)]
    pub dry_run: bool,
    /// Assume yes for confirmations
    #[arg(long, global = true)]
    pub yes: bool,
    /// Never prompt; use defaults and plain output
    #[arg(long, global = true)]
    pub non_interactive: bool,
    /// Tuning profile: safe, server, performance or custom
    #[arg(long, global = true, value_name = "PROFILE")]
    pub profile: Option<String>,
    /// Skip cloud metadata lookups
    #[arg(long, global = true)]
    pub no_imds: bool,
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(visible_aliases = ["f"])]
    Fetch,
    #[command(visible_aliases = ["m", "mon"])]
    Monitor,
    #[command(visible_aliases = ["d", "det"])]
    Detect,
    #[command(visible_aliases = ["a", "an"])]
    Analyze,
    /// Detect, plan, apply and persist adaptive kernel tuning
    #[command(visible_aliases = ["o", "opt"])]
    Optimize {
        #[command(subcommand)]
        action: Option<OptimizeAction>,
        /// Use the balanced Server profile (default)
        #[arg(long, conflicts_with_all = ["safe", "performance"])]
        server: bool,
        /// Use the conservative Safe profile
        #[arg(long, conflicts_with_all = ["server", "performance"])]
        safe: bool,
        /// Use the throughput-oriented Performance profile
        #[arg(long, conflicts_with_all = ["safe", "server"])]
        performance: bool,
        /// Comma-separated categories (memory,limits,network,cpu,io,thp,numa,irq,swap,kernel)
        #[arg(long, value_name = "LIST")]
        categories: Option<String>,
        /// Swap size to create when swap is missing (for example 2G, 4096M or none)
        #[arg(long, value_name = "SIZE")]
        swap: Option<String>,
        /// Opt in to IRQ/RPS/XPS steering on supported bare-metal NICs
        #[arg(long)]
        enable_rps: bool,
    },
    #[command(visible_aliases = ["s", "st"])]
    Status,
    #[command(visible_aliases = ["r", "rep"])]
    Report,
    #[command(visible_aliases = ["hl"])]
    Health,
    #[command(visible_aliases = ["p", "ps", "proc"])]
    Process,
    #[command(visible_aliases = ["n", "net"])]
    Network,
    #[command(visible_aliases = ["stg", "disk"])]
    Storage,
    #[command(visible_aliases = ["mem"])]
    Memory,
    #[command(visible_aliases = ["cpuinfo"])]
    Cpu,
    #[command(visible_aliases = ["svc"])]
    Services,
    #[command(visible_aliases = ["sec"])]
    Security,
    #[command(visible_aliases = ["bench", "bm"])]
    Benchmark {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    #[command(visible_aliases = ["doc", "dr"])]
    Doctor,
    Backup {
        label: Option<String>,
    },
    Backups {
        id: Option<String>,
    },
    #[command(visible_aliases = ["rb", "roll"])]
    Rollback {
        #[arg(long)]
        tx: Option<String>,
    },
    #[command(visible_aliases = ["hist"])]
    History,
    #[command(visible_aliases = ["c", "cfg"])]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    #[command(visible_aliases = ["v", "ver"])]
    Version,
    #[command(visible_aliases = ["un"])]
    Uninstall,
    #[command(visible_aliases = ["up"])]
    Update,
}

#[derive(Subcommand, Debug)]
pub enum OptimizeAction {
    /// Show managed settings, runtime drift, swap and the last transaction
    Status,
    /// Revert a KERNYX transaction and its runtime changes
    Rollback {
        /// Transaction ID (defaults to the most recent one)
        #[arg(long, value_name = "ID")]
        tx: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    Show,
    Validate,
}
