use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
    #[arg(short, long)]
    pub dir: Option<PathBuf>,
}

#[derive(Subcommand, Clone)]
pub enum Command {
    New {
        body: String,
        #[arg(short, long)]
        aliases: Option<Vec<String>>,
    },
    Read {
        id: String,
    },
}
