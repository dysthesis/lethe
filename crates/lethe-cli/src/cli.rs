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
        body: Option<String>,
        #[arg(long, conflicts_with = "body")]
        body_file: Option<PathBuf>,
        #[arg(long, conflicts_with_all = ["body", "body_file"])]
        empty_body: bool,
        #[arg(short, long, value_delimiter = ',')]
        aliases: Option<Vec<String>>,
    },
    Read {
        id: String,
    },
    Edit {
        id: String,
        #[arg(long, conflicts_with = "body_file")]
        body: Option<String>,
        #[arg(long, conflicts_with = "body")]
        body_file: Option<PathBuf>,
        #[arg(long, conflicts_with_all = ["body", "body_file"])]
        empty_body: bool,
        #[arg(long, value_delimiter = ',', conflicts_with = "clear_aliases")]
        aliases: Option<Vec<String>>,
        #[arg(long, conflicts_with = "aliases")]
        clear_aliases: bool,
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        #[arg(long = "unset", value_name = "KEY")]
        unset: Vec<String>,
        #[arg(long)]
        clear_extra: bool,
    },
}
