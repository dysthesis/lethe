use std::{fs, io, path::PathBuf};

use chrono::{DateTime, Utc};
#[cfg(all(test, feature = "arbitrary"))]
use proptest_derive::Arbitrary;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identifier::Identifier;

/// A note is physically represented as a directory consisting of
///
/// - a `meta.toml` serialising its metadata,
/// - a `body.md` containing its text body, and
/// - arbitrary attachments.
///
#[derive(Default, Debug)]
#[cfg_attr(all(test, feature = "arbitrary"), derive(Arbitrary))]
pub struct Note {
    /// The metadata stored in the note's `meta.toml`
    meta: Metadata,
    /// The note's body, stored in `body.md`
    body: String,
}

#[derive(Error, Debug)]
pub enum NoteError {
    #[error("Failed to read note {id}'s `meta.toml`: {error}")]
    MetadataReadError { id: Identifier, error: io::Error },
    #[error("Failed to parse the metadata for note {id}: {error}")]
    MetadataParseError {
        id: Identifier,
        error: toml::de::Error,
    },
    #[error("Failed to serialise metadata for note {id}: {error}")]
    MetadataSerialiseError {
        id: Identifier,
        error: toml::ser::Error,
    },
    #[error("Failed to read note {id}'s body at `body.md`: {error}")]
    BodyReadError { id: Identifier, error: io::Error },
    #[error("Failed to create directory for note {id}: {error}")]
    NoteCreateDirError { id: Identifier, error: io::Error },
    #[error("Failed to write `meta.toml` for note {id}: {error}")]
    MetadataWriteError { id: Identifier, error: io::Error },
    #[error("Failed to write `body.md` for note {id}: {error}")]
    BodyWriteError { id: Identifier, error: io::Error },
}

impl Note {
    /// Create a new note with the given body and list of aliases
    pub fn new(root: PathBuf, body: String, aliases: Vec<String>) -> Result<Self, NoteError> {
        let id = Identifier::new();
        let dir_path = root.join(id.to_string());
        fs::create_dir_all(&dir_path).map_err(|error| NoteError::NoteCreateDirError {
            id: id.clone(),
            error,
        })?;
        let ctime = Utc::now();
        let mtime = ctime;
        let meta = Metadata {
            id: id.clone(),
            ctime,
            mtime,
            aliases,
        };
        let meta_serialised =
            toml::to_string(&meta).map_err(|error| NoteError::MetadataSerialiseError {
                id: id.clone(),
                error,
            })?;
        let meta_path = dir_path.join("meta.toml");
        fs::write(meta_path, meta_serialised).map_err(|error| NoteError::MetadataWriteError {
            id: id.clone(),
            error,
        })?;
        let body_path = dir_path.join("body.md");
        fs::write(body_path, body.clone())
            .map_err(|error| NoteError::BodyWriteError { id, error })?;
        Ok(Self { body, meta })
    }
    /// Read a note of the given `id` from `root`
    pub fn read(id: Identifier, root: PathBuf) -> Result<Self, NoteError> {
        let dir = root.join(id.to_string());
        let meta_path = dir.join("meta.toml");
        let body_path = dir.join("body.md");
        let meta = match fs::read_to_string(meta_path) {
            Ok(val) => match toml::from_str::<Metadata>(&val) {
                Ok(val) => val,
                Err(error) => return Err(NoteError::MetadataParseError { id, error }),
            },

            Err(error) => return Err(NoteError::MetadataReadError { id, error }),
        };
        let body = fs::read_to_string(body_path)
            .map_err(|error| NoteError::BodyReadError { id, error })?;
        Ok(Self { meta, body })
    }

    /// Get the note's metadata
    pub fn meta(&self) -> &Metadata {
        &self.meta
    }

    /// Get the note's body
    pub fn body(&self) -> &str {
        &self.body
    }
}

/// The metadata stored in the note's `meta.toml`. Consists of at least
///
/// - its creation time (`ctime`),
/// - last modified time (`mtime`), and
/// - a possibly-empty list of aliases (`aliases`),
///
/// as well as any arbitrary key-value pair in addition to the above.
#[cfg_attr(all(test, feature = "arbitrary"), derive(Arbitrary))]
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Metadata {
    /// The note's unique, stable, immutable identifier.
    id: Identifier,
    /// The time when the note was created.
    ctime: DateTime<Utc>,
    /// The time when the note was last modified.
    mtime: DateTime<Utc>,
    /// Aliases to optionally refer to this note by
    // TODO: See if we can work with borrows here
    aliases: Vec<String>,
    // TODO: Store other key-value pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::tempdir;

    proptest! {
        #[test]
        fn note_round_trips(
            body in proptest::collection::vec(any::<char>(), 0..256)
                .prop_map(|chars| chars.into_iter().collect::<String>()),
            aliases in proptest::collection::vec(
                proptest::collection::vec(any::<char>(), 0..64)
                    .prop_map(|chars| chars.into_iter().collect::<String>()),
                0..8,
            ),
        ) {
            let dir = tempdir().unwrap();
            let root = dir.path().to_path_buf();

            let note = Note::new(root.clone(), body.clone(), aliases.clone()).unwrap();
            let reread = Note::read(note.meta.id.clone(), root).unwrap();

            prop_assert_eq!(reread.body, body);
            prop_assert_eq!(reread.meta.aliases, aliases);
        }
    }
}
