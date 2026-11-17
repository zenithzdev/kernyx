use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "kernyx",
    version = "2.0.0",
    about = "KERNYX - Adaptive Linux Performance Optimization Engine"
)]
pub struct Cli {
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true)]
    pub quiet: bool,
    #[arg(long, global = true)]
    pub verbose: bool,
    #[arg(long, global = true)]
    pub dry_run: bool,
    #[arg(long, global = true)]
    pub yes: bool,
    #[arg(long, global = true)]
    pub profile: Option<String>,
    #[arg(long, global = true)]
    pub no_imds: bool,
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(aliases = ["f"], visible_aliases = ["f"])]
    Fetch,
    #[command(aliases = ["m", "mon"], visible_aliases = ["m", "mon"])]
    Monitor,
    #[command(aliases = ["d", "det"], visible_aliases = ["d", "det"])]
    Detect,
    #[command(aliases = ["a", "an"], visible_aliases = ["a", "an"])]
    Analyze,
    #[command(aliases = ["o", "opt"], visible_aliases = ["o", "opt"])]
    Optimize {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    #[command(aliases = ["s", "st"], visible_aliases = ["s", "st"])]
    Status,
    #[command(aliases = ["r", "rep"], visible_aliases = ["r", "rep"])]
    Report,
    #[command(aliases = ["hl"], visible_aliases = ["hl"])]
    Health,
    #[command(aliases = ["p", "ps", "proc"], visible_aliases = ["p", "ps", "proc"])]
    Process,
    #[command(aliases = ["n", "net"], visible_aliases = ["n", "net"])]
    Network,
    #[command(aliases = ["stg", "disk"], visible_aliases = ["stg", "disk"])]
    Storage,
    #[command(aliases = ["mem"], visible_aliases = ["mem"])]
    Memory,
    #[command(aliases = ["cpuinfo"], visible_aliases = ["cpuinfo"])]
    Cpu,
    #[command(aliases = ["svc"], visible_aliases = ["svc"])]
    Services,
    #[command(aliases = ["sec"], visible_aliases = ["sec"])]
    Security,
    #[command(aliases = ["bench", "bm"], visible_aliases = ["bench", "bm"])]
    Benchmark {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    #[command(aliases = ["doc", "dr"], visible_aliases = ["doc", "dr"])]
    Doctor,
    #[command(aliases = ["backup"], visible_aliases = ["backup"])]
    Backup { label: Option<String> },
    #[command(aliases = ["backups"], visible_aliases = ["backups"])]
    Backups { id: Option<String> },
    #[command(aliases = ["rb", "roll"], visible_aliases = ["rb", "roll"])]
    Rollback {
        #[arg(long)]
        tx: Option<String>,
    },
    #[command(aliases = ["h", "hist"], visible_aliases = ["h", "hist"])]
    History,
    #[command(aliases = ["c", "cfg"], visible_aliases = ["c", "cfg"])]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    #[command(aliases = ["v", "ver"], visible_aliases = ["v", "ver"])]
    Version,
    #[command(aliases = ["un"], visible_aliases = ["un"])]
    Uninstall,
    #[command(aliases = ["up"], visible_aliases = ["up"])]
    Update,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    Show,
    Validate,
}
