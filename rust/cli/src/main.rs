//! The tandem binary: the agent face, and later the `rpc` the app holds.
//!
//! One binary (`docs/port.md`, Target shape): the verbs agents call and the
//! core the app trusts are the same executable, so they cannot drift. This rung
//! carries the home plumbing and `doctor`'s pinned report with its `home` row:
//! the verbs land with PRT-9 and the other seven checks with PRT-10.
//!
//! What lives here is what belongs to a process rather than to the domain:
//! argv, the environment, the two streams and the exit code.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde::Serialize;
use tandem_core::store::{self, Home};
use tandem_core::wire::{DoctorCheck, DoctorReport};

#[derive(Parser)]
#[command(
    name = "tandem",
    version,
    about = "Tandem: the board, the bikeshed and the records"
)]
struct Cli {
    /// The home this run keeps its state in (default: $TANDEM_HOME, else ~/.tandem).
    #[arg(long, global = true, value_name = "PATH")]
    home: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Report whether this machine's Tandem setup is sound (ADR 0014).
    Doctor,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let user_home = store::user_home().map_err(|error| error.to_string())?;
    let environment = env::var(store::ENVIRONMENT).ok();
    let home = Home::resolve(cli.home.as_deref(), environment.as_deref(), &user_home);

    match cli.command {
        Command::Doctor => print(&report(&home)),
    }
}

/// ADR 0014's report, with the one row this rung can answer: `home` names the
/// directory the run resolved and why. `home` is informational, so it cannot
/// fail and the report is `ok`; a writing verb creates the home, never this
/// read-only one.
fn report(home: &Home) -> DoctorReport {
    DoctorReport {
        ok: true,
        checks: vec![DoctorCheck {
            name: "home".to_string(),
            ok: true,
            detail: format!(
                "{} ({})",
                home.directory().display(),
                home.source().as_str()
            ),
        }],
    }
}

/// Success is exactly one JSON object on stdout, one trailing newline.
fn print<T: Serialize>(value: &T) -> Result<(), String> {
    let line = serde_json::to_string(value).map_err(|error| error.to_string())?;
    println!("{line}");
    Ok(())
}
