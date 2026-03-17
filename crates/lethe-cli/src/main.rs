use std::{env, str::FromStr};

use clap::Parser;
use color_eyre::eyre::eyre;
use lethe_core::{identifier::Identifier, note::Note};

use crate::cli::Cli;

mod cli;
fn main() -> color_eyre::eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    let root = cli.dir.or_else(|| env::current_dir().ok()).ok_or_else(|| eyre!("Could not determine current working directory, and none was provided in the command-line arguments!"))?;
    let res = match cli.command {
        cli::Command::New { body, aliases } => Note::new(root, body, aliases.unwrap_or_default()),
        cli::Command::Read { id } => {
            let trimmed = id.trim();
            Note::read(Identifier::from_str(trimmed)?, root)
        }
    }?;

    println!("Note: {res:?}");

    Ok(())
}
