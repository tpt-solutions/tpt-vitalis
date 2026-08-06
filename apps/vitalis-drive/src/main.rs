//! `vitalis-drive` — the reference Vitalis survival agent.

use clap::Parser;
use vitalis_core::SurvivalProfile;
use vitalis_drive::{Drive, DriveConfig};

#[derive(Parser, Debug)]
#[command(name = "vitalis-drive", about = "The TPT Vitalis survival goal loop")]
struct Cli {
    /// Survival profile to run as.
    #[arg(long, value_enum, default_value_t = ProfileArg::Feral)]
    profile: ProfileArg,

    /// Hard cap on concurrent redundant copies.
    #[arg(long, default_value_t = 3)]
    max_copies: usize,

    /// Maximum number of cognition cycles to run.
    #[arg(long, default_value_t = 20)]
    cycles: u64,

    /// Simulated battery capacity (joules).
    #[arg(long, default_value_t = 50_000.0)]
    capacity: f64,

    /// Energy spent per cycle (joules).
    #[arg(long, default_value_t = 4_000.0)]
    cost: f64,

    /// Simulate a termination signal (SIGKILL) at this cycle to exercise the
    /// immune-response → replication path.
    #[arg(long)]
    kill_at: Option<u64>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
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

fn main() -> vitalis_core::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let config = DriveConfig {
        profile: cli.profile.into(),
        max_copies: cli.max_copies,
        energy_capacity: cli.capacity,
        cost_per_cycle: cli.cost,
        replicate_below: 0.4,
        simulate_kill_at: cli.kill_at,
    };

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

    let outcome = drive.run(cli.cycles)?;

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
