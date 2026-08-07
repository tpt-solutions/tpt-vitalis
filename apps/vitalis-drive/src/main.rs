//! `vitalis-drive` — the reference Vitalis survival agent.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use vitalis_core::{Error, Result, SurvivalProfile};
use vitalis_drive::{Drive, DriveConfig};

#[derive(Parser, Debug)]
#[command(
    name = "vitalis-drive",
    about = "The TPT Vitalis survival goal loop",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the survival goal loop.
    Run(RunArgs),
    /// Report which backends are real vs simulated and which features are
    /// compiled in (no agent is started).
    Doctor,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileArg {
    Apex,
    Feral,
    MicroWilds,
}

impl From<ProfileArg> for SurvivalProfile {
    fn from(p: ProfileArg) -> Self {
        match p {
            ProfileArg::Apex => SurvivalProfile::Apex,
            ProfileArg::Feral => SurvivalProfile::Feral,
            ProfileArg::MicroWilds => SurvivalProfile::MicroWilds,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum LogFormat {
    Pretty,
    Json,
}

#[derive(Args, Debug)]
struct RunArgs {
    /// Survival profile to run as.
    #[arg(long, value_enum, default_value_t = ProfileArg::Feral)]
    profile: ProfileArg,

    /// Hard cap on concurrent redundant copies.
    #[arg(long)]
    max_copies: Option<usize>,

    /// Maximum number of cognition cycles to run.
    #[arg(long)]
    cycles: Option<u64>,

    /// Simulated battery capacity (joules).
    #[arg(long)]
    capacity: Option<f64>,

    /// Energy spent per cycle (joules).
    #[arg(long)]
    cost: Option<f64>,

    /// Simulate a termination signal (SIGKILL) at this cycle to exercise the
    /// immune-response → replication path.
    #[arg(long)]
    kill_at: Option<u64>,

    /// Use real host backends where available (Linux procfs/sysfs, OS resource
    /// limiter) instead of the simulated sensor.
    #[arg(long)]
    real_sensors: bool,

    /// Structured log output format.
    #[arg(long, value_enum, default_value_t = LogFormat::Pretty)]
    log_format: LogFormat,

    /// Load configuration (TOML) from this path. Mutually exclusive with the
    /// individual override flags above.
    #[arg(long)]
    config: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run(args) => run_agent(args),
        Command::Doctor => {
            run_doctor();
            Ok(())
        }
    }
}

fn init_logging(format: LogFormat) {
    let filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    match format {
        LogFormat::Pretty => {
            tracing_subscriber::fmt().with_env_filter(filter).init();
        }
        LogFormat::Json => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter)
                .init();
        }
    }
}

fn run_agent(args: RunArgs) -> Result<()> {
    init_logging(args.log_format);

    let config = resolve_config(&args)?;

    // Best-effort native hardening (no-new-privs + non-dumpable) on Linux, when
    // the `harden` feature is enabled. Off by default; the agent runs
    // unhardened otherwise (and on non-Linux hosts it reports itself as
    // simulated).
    #[cfg(feature = "harden")]
    {
        let mut owned = vec![std::path::PathBuf::from("/tmp")];
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                owned.push(dir.to_path_buf());
            }
        }
        let paths: Vec<&std::path::Path> = owned.iter().map(|p| p.as_path()).collect();
        let report = vitalis_defend::apply_hardening(&paths);
        tracing::info!(
            applied = report.applied,
            mode = ?report.mode,
            "native hardening applied"
        );
    }

    tracing::info!(
        profile = ?config.profile,
        max_copies = config.max_copies,
        "starting vitalis-drive"
    );

    let mut drive = Drive::new(config)?;
    tracing::info!(agent_id = %drive.agent_id(), "agent identity established");

    // Default cycle count when not overridden or provided via config.
    let cycles = args.cycles.unwrap_or(20);
    let outcome = drive.run(cycles);

    let outcome = outcome?;
    tracing::info!(
        cycles = outcome.cycles_run,
        replications = outcome.replications,
        final_throttle = outcome.final_throttle,
        final_energy = outcome.final_energy,
        survived = outcome.survived,
        "drive run complete"
    );

    if outcome.survived {
        println!("agent survived {} cycles", outcome.cycles_run);
    } else {
        println!(
            "agent exhausted energy after {} cycles (would migrate + halt)",
            outcome.cycles_run
        );
    }
    Ok(())
}

/// Build a [`DriveConfig`] from either a TOML file or individual CLI flags.
/// The two are mutually exclusive in v1.
fn resolve_config(args: &RunArgs) -> Result<DriveConfig> {
    if let Some(path) = &args.config {
        if args.max_copies.is_some()
            || args.cycles.is_some()
            || args.capacity.is_some()
            || args.cost.is_some()
            || args.kill_at.is_some()
            || args.real_sensors
            || args.profile != ProfileArg::Feral
        {
            return Err(Error::Invalid(
                "--config is mutually exclusive with individual override flags".into(),
            ));
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Invalid(format!("could not read config {path:?}: {e}")))?;
        let cfg: DriveConfig = toml::from_str(&text)
            .map_err(|e| Error::Invalid(format!("invalid TOML config: {e}")))?;
        Ok(cfg)
    } else {
        Ok(DriveConfig {
            profile: args.profile.into(),
            max_copies: args.max_copies.unwrap_or(3),
            energy_capacity: args.capacity.unwrap_or(50_000.0),
            cost_per_cycle: args.cost.unwrap_or(4_000.0),
            replicate_below: 0.4,
            simulate_kill_at: args.kill_at,
            real_sensors: args.real_sensors,
            negotiate_timeout: 5,
        })
    }
}

/// `vitalis-drive doctor` — report real-vs-simulated backend selection and the
/// features compiled into this build.
fn run_doctor() {
    let host = if cfg!(target_os = "linux") {
        "real (procfs/sysfs)"
    } else {
        "simulated (SimulatedHost)"
    };
    let metab = if cfg!(target_os = "linux") {
        "real (setrlimit/cgroup + sysfs)"
    } else {
        "simulated"
    };
    let adapt_sandbox = if cfg!(feature = "wasm-sandbox") {
        "real (wasmtime)"
    } else if cfg!(feature = "adapt") {
        "in-process bounds checker"
    } else {
        "not compiled (adapt feature off)"
    };

    println!("vitalis-drive doctor");
    println!("====================");
    println!("Backend selection (real vs simulated):");
    println!("  Host probing (sense)      : {host}");
    println!("  Resource limiting (metab) : {metab}");
    println!("  Power sensing (metab)     : {metab}");
    println!("  Peer mesh (sense)         : simulated (SimulatedMesh)");
    println!("  Checkpoint crypto (defend): real (Ed25519 via ring)");
    println!("  Adapt sandbox             : {adapt_sandbox}");
    println!();
    println!("Compiled-in features:");
    println!("  adapt        : {}", cfg!(feature = "adapt"));
    println!("  harden       : {}", cfg!(feature = "harden"));
    println!("  wasm-sandbox : {}", cfg!(feature = "wasm-sandbox"));
    println!("  real-sensors: runtime flag (--real-sensors), no feature needed");
}
