use std::{
    env, fs,
    io::{self, IsTerminal, Read},
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    str::FromStr,
};

use clap::Parser;
use color_eyre::eyre::eyre;
use lethe_core::{
    identifier::Identifier,
    note::Note,
    repository::{AliasesEdit, BodyEdit, CreateSpec, EditSpec, ExtraEdit, Repository},
};
use tempfile::Builder;

use crate::cli::Cli;

mod cli;
fn main() -> color_eyre::eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();
    let root = cli.dir.or_else(|| env::current_dir().ok()).ok_or_else(|| eyre!("Could not determine current working directory, and none was provided in the command-line arguments!"))?;
    let stdin_is_piped = !io::stdin().is_terminal();
    let res = match cli.command {
        cli::Command::New {
            body,
            body_file,
            empty_body,
            aliases,
        } => {
            let mut repo = Repository::open(root.clone());
            let body = resolve_new_body(body, body_file, empty_body, stdin_is_piped)?;
            repo.create_note(CreateSpec::new(body, aliases.unwrap_or_default()))
        }
        cli::Command::Read { id } => {
            let mut repo = Repository::open(root.clone());
            let trimmed = id.trim();
            repo.read_note(Identifier::from_str(trimmed)?)
        }
        cli::Command::Edit {
            id,
            body,
            body_file,
            empty_body,
            aliases,
            clear_aliases,
            set,
            unset,
            clear_extra,
        } => {
            let mut repo = Repository::open(root.clone());
            let trimmed = id.trim();
            let mut note = repo.read_note(Identifier::from_str(trimmed)?)?;

            let has_other_edits = aliases.is_some()
                || clear_aliases
                || !set.is_empty()
                || !unset.is_empty()
                || clear_extra;

            let body = resolve_edit_body(
                &note,
                body,
                body_file,
                has_other_edits,
                empty_body,
                stdin_is_piped,
            )?;

            let body_edit = match body {
                Some(body) => BodyEdit::Replace(body),
                None => BodyEdit::Keep,
            };
            let aliases_edit = if clear_aliases {
                AliasesEdit::Clear
            } else if let Some(aliases) = aliases {
                AliasesEdit::Replace(aliases)
            } else {
                AliasesEdit::Keep
            };
            let extra = ExtraEdit::from_raw(set, unset, clear_extra)?;

            let spec = EditSpec::new(body_edit, aliases_edit, extra);
            let _changed = repo.edit_loaded_note(&mut note, spec)?;

            Ok(note)
        }
    }?;

    println!("Note: {res:?}");

    Ok(())
}

fn resolve_new_body(
    body: Option<String>,
    body_file: Option<PathBuf>,
    empty_body: bool,
    stdin_is_piped: bool,
) -> color_eyre::eyre::Result<String> {
    if empty_body {
        if body.is_some() || body_file.is_some() {
            return Err(eyre!(
                "--empty-body cannot be combined with --body or --body-file"
            ));
        }
        if stdin_is_piped {
            return Err(eyre!("--empty-body cannot be used when stdin is piped"));
        }
        return Ok(String::new());
    }
    if let Some(body) = body {
        return ensure_body(body);
    }
    if let Some(path) = body_file {
        return read_body_from_file(&path).and_then(ensure_body);
    }
    if stdin_is_piped {
        return read_body_from_stdin().and_then(ensure_body);
    }
    open_editor("")
}

fn resolve_edit_body(
    note: &Note,
    body: Option<String>,
    body_file: Option<PathBuf>,
    has_other_edits: bool,
    empty_body: bool,
    stdin_is_piped: bool,
) -> color_eyre::eyre::Result<Option<String>> {
    if empty_body {
        if body.is_some() || body_file.is_some() {
            return Err(eyre!(
                "--empty-body cannot be combined with --body or --body-file"
            ));
        }
        if stdin_is_piped {
            return Err(eyre!("--empty-body cannot be used when stdin is piped"));
        }
        return Ok(Some(String::new()));
    }
    if let Some(body) = body {
        return ensure_body(body).map(Some);
    }
    if let Some(path) = body_file {
        return read_body_from_file(&path)
            .and_then(ensure_body)
            .map(Some);
    }
    if stdin_is_piped {
        return read_body_from_stdin()
            .and_then(ensure_body)
            .map(Some);
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

fn ensure_body(body: String) -> color_eyre::eyre::Result<String> {
    if body.trim().is_empty() {
        Err(eyre!(
            "No note body was provided; use --empty-body to permit an empty body"
        ))
    } else {
        Ok(body)
    }
}

fn open_editor(initial: &str) -> color_eyre::eyre::Result<String> {
    let temp = Builder::new()
        .suffix(".md")
        .tempfile()
        .map_err(|error| eyre!("Failed to create temp file: {error}"))?;
    fs::write(temp.path(), initial)
        .map_err(|error| eyre!("Failed to write editor temp file: {error}"))?;

    let editor = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let mut parts = shell_words::split(&editor)
        .map_err(|error| eyre!("Failed to parse $EDITOR `{editor}`: {error}"))?;
    let command = parts
        .first()
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

    let body = fs::read_to_string(temp.path())
        .map_err(|error| eyre!("Failed to read editor temp file: {error}"))?;
    ensure_body(body)
}
