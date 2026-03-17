use std::{
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    str::FromStr,
};

use clap::Parser;
use color_eyre::eyre::eyre;
use lethe_core::{identifier::Identifier, note::Note};
use tempfile::NamedTempFile;

use crate::cli::Cli;

mod cli;
fn main() -> color_eyre::eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    let root = cli.dir.or_else(|| env::current_dir().ok()).ok_or_else(|| eyre!("Could not determine current working directory, and none was provided in the command-line arguments!"))?;
    let res = match cli.command {
        cli::Command::New {
            body,
            body_file,
            body_stdin,
            aliases,
        } => {
            let body = resolve_new_body(body, body_file, body_stdin)?;
            Note::new(&root, body, aliases.unwrap_or_default())
        }
        cli::Command::Read { id } => {
            let trimmed = id.trim();
            Note::read(Identifier::from_str(trimmed)?, root)
        }
        cli::Command::Edit {
            id,
            body,
            body_file,
            body_stdin,
            aliases,
            clear_aliases,
            set,
            unset,
            clear_extra,
        } => {
            let trimmed = id.trim();
            let mut note = Note::read(Identifier::from_str(trimmed)?, root.clone())?;

            if clear_extra && (!set.is_empty() || !unset.is_empty()) {
                return Err(eyre!(
                    "--clear-extra cannot be combined with --set or --unset"
                ));
            }

            let set_pairs = parse_set_pairs(&set)?;
            validate_extra_keys(set_pairs.iter().map(|(key, _)| key.as_str()))?;
            validate_extra_keys(unset.iter().map(|key| key.as_str()))?;

            let has_other_edits =
                aliases.is_some() || clear_aliases || !set_pairs.is_empty() || !unset.is_empty()
                    || clear_extra;

            let body = resolve_edit_body(
                &note,
                body,
                body_file,
                body_stdin,
                has_other_edits,
            )?;

            let changed = note.update(|edit| {
                if let Some(body) = body {
                    edit.set_body(body);
                }
                if clear_aliases {
                    edit.set_aliases(Vec::new());
                }
                if let Some(aliases) = aliases {
                    edit.set_aliases(aliases);
                }
                if clear_extra {
                    edit.clear_extra();
                }
                for (key, value) in set_pairs {
                    edit.set_extra_value(key, value)?;
                }
                for key in unset {
                    edit.remove_extra_key(&key)?;
                }
                Ok(())
            })?;

            if changed {
                note.write(&root)?;
            }

            Ok(note)
        }
    }?;

    println!("Note: {res:?}");

    Ok(())
}

const RESERVED_KEYS: [&str; 4] = ["id", "ctime", "mtime", "aliases"];

fn is_reserved_key(key: &str) -> bool {
    RESERVED_KEYS.iter().any(|reserved| reserved == &key)
}

fn validate_extra_keys<'a>(
    keys: impl Iterator<Item = &'a str>,
) -> color_eyre::eyre::Result<()> {
    for key in keys {
        if is_reserved_key(key) {
            return Err(eyre!("Metadata key `{key}` is reserved and cannot be set via extra"));
        }
        if key.contains('.') {
            return Err(eyre!(
                "Metadata key `{key}` is not supported: nested keys are not yet supported"
            ));
        }
        if key.trim().is_empty() {
            return Err(eyre!("Metadata key cannot be empty"));
        }
    }
    Ok(())
}

fn parse_set_pairs(items: &[String]) -> color_eyre::eyre::Result<Vec<(String, toml::Value)>> {
    let mut pairs = Vec::with_capacity(items.len());
    for item in items {
        let (key, raw_value) = item
            .split_once('=')
            .ok_or_else(|| eyre!("Invalid --set value `{item}`; expected KEY=VALUE"))?;
        let key = key.trim();
        let raw_value = raw_value.trim();
        if key.is_empty() {
            return Err(eyre!("Invalid --set value `{item}`; key is empty"));
        }
        if raw_value.is_empty() {
            return Err(eyre!("Invalid --set value `{item}`; value is empty"));
        }
        let snippet = format!("{key} = {raw_value}");
        let table: toml::Table = toml::from_str(&snippet).map_err(|error| {
            eyre!(
                "Invalid TOML value in --set `{item}`: {error}. Try quoting strings."
            )
        })?;
        let value = table.get(key).ok_or_else(|| {
            eyre!(
                "Invalid --set value `{item}`; only simple top-level keys are supported"
            )
        })?;
        pairs.push((key.to_string(), value.clone()));
    }
    Ok(pairs)
}

fn resolve_new_body(
    body: Option<String>,
    body_file: Option<PathBuf>,
    body_stdin: bool,
) -> color_eyre::eyre::Result<String> {
    if let Some(body) = body {
        return Ok(body);
    }
    if let Some(path) = body_file {
        return read_body_from_file(&path);
    }
    if body_stdin {
        return read_body_from_stdin();
    }
    open_editor("")
}

fn resolve_edit_body(
    note: &Note,
    body: Option<String>,
    body_file: Option<PathBuf>,
    body_stdin: bool,
    has_other_edits: bool,
) -> color_eyre::eyre::Result<Option<String>> {
    if let Some(body) = body {
        return Ok(Some(body));
    }
    if let Some(path) = body_file {
        return read_body_from_file(&path).map(Some);
    }
    if body_stdin {
        return read_body_from_stdin().map(Some);
    }
    if has_other_edits {
        return Ok(None);
    }
    open_editor(note.body()).map(Some)
}

fn read_body_from_file(path: &Path) -> color_eyre::eyre::Result<String> {
    fs::read_to_string(path)
        .map_err(|error| eyre!("Failed to read body from `{}`: {error}", path.display()))
}

fn read_body_from_stdin() -> color_eyre::eyre::Result<String> {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|error| eyre!("Failed to read body from stdin: {error}"))?;
    Ok(buffer)
}

fn open_editor(initial: &str) -> color_eyre::eyre::Result<String> {
    let temp = NamedTempFile::new().map_err(|error| eyre!("Failed to create temp file: {error}"))?;
    fs::write(temp.path(), initial)
        .map_err(|error| eyre!("Failed to write editor temp file: {error}"))?;

    let editor = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let mut parts = shell_words::split(&editor)
        .map_err(|error| eyre!("Failed to parse $EDITOR `{editor}`: {error}"))?;
    let command = parts
        .get(0)
        .ok_or_else(|| eyre!("$EDITOR is empty; set it to your preferred editor"))?
        .to_string();
    let args = parts.drain(1..).collect::<Vec<_>>();

    let status = ProcessCommand::new(command)
        .args(args)
        .arg(temp.path())
        .status()
        .map_err(|error| eyre!("Failed to launch editor `{editor}`: {error}"))?;
    if !status.success() {
        return Err(eyre!("Editor exited with status {status}"));
    }

    fs::read_to_string(temp.path())
        .map_err(|error| eyre!("Failed to read editor temp file: {error}"))
}
