use std::{env, str::FromStr};

use clap::Parser;
use lethe_core::{identifier::Identifier, note::Note};

use crate::cli::Cli;

mod cli;
fn main() -> color_eyre::eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    // TODO: FIX UNWRAP!!!
    let root = cli.dir.or_else(|| env::current_dir().ok()).unwrap();
    let res = match cli.command {
        cli::Command::New { body, aliases } => Note::new(root, body, aliases.unwrap_or_default()),
        cli::Command::Read { id } => Note::read(Identifier::from_str(&id)?, root),
    }?;

    println!("Note: {res:?}");

    Ok(())
}
