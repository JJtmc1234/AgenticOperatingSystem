//! The fixed launcher for Evan's governed issue workflow.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, ExitStatus};

use anyhow::{Context, Result};

fn command(program: &Path, home: &Path, args: &[OsString]) -> Result<Command> {
    anyhow::ensure!(
        program.is_file(),
        "Evan workflow is not installed at {}. Run integrations/evan/install.sh from AOS.",
        program.display()
    );
    let mut command = Command::new(program);
    command.arg("--home").arg(home.join("evan")).args(args);
    Ok(command)
}

pub fn run(home: &Path, args: &[OsString]) -> Result<ExitStatus> {
    let user_home =
        std::env::var_os("HOME").context("HOME is required to locate the Evan launcher")?;
    let program = Path::new(&user_home).join(".local/bin/aos-evan");
    command(&program, home, args)?
        .status()
        .with_context(|| format!("could not start Evan workflow at {}", program.display()))
}

#[cfg(test)]
#[path = "evan_tests.rs"]
mod tests;
