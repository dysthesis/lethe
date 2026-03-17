use std::{
    fs, io,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
#[cfg(all(test, feature = "arbitrary"))]
use proptest_derive::Arbitrary;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identifier::Identifier;

const RESERVED_KEYS: [&str; 4] = ["id", "ctime", "mtime", "aliases"];

fn is_reserved_key(key: &str) -> bool {
    RESERVED_KEYS.iter().any(|reserved| reserved == &key)
}

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
    /// Whether the body needs to be persisted.
    dirty_body: bool,
    /// Whether the metadata needs to be persisted.
    dirty_meta: bool,
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

#[derive(Error, Debug)]
pub enum MetadataError {
    #[error("Metadata key `{key}` is reserved and cannot be set via `extra`")]
    ReservedKey { key: String },
}

impl Note {
    /// Create a new note with the given body and list of aliases
    pub fn new(root: &Path, body: String, aliases: Vec<String>) -> Result<Self, NoteError> {
        let id = Identifier::new();

        let ctime = Utc::now();
        let mtime = ctime;
        let meta = Metadata {
            id: id.clone(),
            ctime,
            mtime,
            aliases,
            extra: toml::Table::new(),
        };

        let mut note = Self {
            body,
            meta,
            dirty_body: true,
            dirty_meta: true,
        };
        note.write(root)?;

        Ok(note)
    }

    /// Write the current note into its file
    pub fn write(&mut self, root: &Path) -> Result<(), NoteError> {
        if !self.dirty_body && !self.dirty_meta {
            return Ok(());
        }

        let dir_path = root.join(self.meta.id.to_string());
        fs::create_dir_all(&dir_path).map_err(|error| NoteError::NoteCreateDirError {
            id: self.meta.id.clone(),
            error,
        })?;

        if self.dirty_meta {
            let meta_serialised =
                toml::to_string(&self.meta).map_err(|error| NoteError::MetadataSerialiseError {
                    id: self.meta.id.clone(),
                    error,
                })?;
            let meta_path = dir_path.join("meta.toml");

            fs::write(meta_path, meta_serialised).map_err(|error| {
                NoteError::MetadataWriteError {
                    id: self.meta.id.clone(),
                    error,
                }
            })?;
            self.dirty_meta = false;
        }

        if self.dirty_body {
            let body_path = dir_path.join("body.md");
            fs::write(body_path, self.body.clone()).map_err(|error| NoteError::BodyWriteError {
                id: self.meta.id.clone(),
                error,
            })?;
            self.dirty_body = false;
        }

        Ok(())
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
        Ok(Self {
            meta,
            body,
            dirty_body: false,
            dirty_meta: false,
        })
    }

    pub fn update(
        &mut self,
        f: impl FnOnce(&mut NoteEditor<'_>) -> Result<(), MetadataError>,
    ) -> Result<bool, MetadataError> {
        let mut editor = NoteEditor::new(self);
        f(&mut editor)?;
        let mutated = editor.mutated;
        let dirty_body = editor.dirty_body;
        let dirty_meta = editor.dirty_meta;
        if mutated {
            self.dirty_body |= dirty_body;
            self.dirty_meta |= dirty_meta;
            self.touch();
        }
        Ok(mutated)
    }

    pub fn set_body(&mut self, body: String) {
        if self.body == body {
            return;
        }
        self.body = body;
        self.dirty_body = true;
        self.touch();
    }

    /// Get the note's metadata
    pub fn meta(&self) -> &Metadata {
        &self.meta
    }

    /// Get the note's body
    pub fn body(&self) -> &str {
        &self.body
    }

    fn touch(&mut self) {
        self.meta.mtime = Utc::now();
        self.dirty_meta = true;
    }
}

/// Struct to keep track of the mutations to `Note`, in order to batch together
/// mutations before writing.
pub struct NoteEditor<'a> {
    note: &'a mut Note,
    mutated: bool,
    dirty_body: bool,
    dirty_meta: bool,
}

impl<'a> NoteEditor<'a> {
    fn new(note: &'a mut Note) -> Self {
        Self {
            note,
            mutated: false,
            dirty_body: false,
            dirty_meta: false,
        }
    }

    pub fn body(&self) -> &str {
        &self.note.body
    }

    pub fn meta(&self) -> &Metadata {
        &self.note.meta
    }

    pub fn set_body(&mut self, body: impl Into<String>) -> bool {
        let body = body.into();
        if self.note.body == body {
            return false;
        }
        self.note.body = body;
        self.dirty_body = true;
        self.mutated = true;
        true
    }

    pub fn set_aliases(&mut self, aliases: Vec<String>) -> bool {
        if self.note.meta.aliases == aliases {
            return false;
        }
        self.note.meta.aliases = aliases;
        self.dirty_meta = true;
        self.mutated = true;
        true
    }

    pub fn set_extra_value(
        &mut self,
        key: impl Into<String>,
        value: toml::Value,
    ) -> Result<bool, MetadataError> {
        let key = key.into();
        if is_reserved_key(&key) {
            return Err(MetadataError::ReservedKey { key });
        }
        let changed = match self.note.meta.extra.get(&key) {
            Some(existing) => existing != &value,
            None => true,
        };
        if changed {
            self.note.meta.extra.insert(key, value);
            self.dirty_meta = true;
            self.mutated = true;
        }
        Ok(changed)
    }

    pub fn remove_extra_key(&mut self, key: &str) -> Result<bool, MetadataError> {
        if is_reserved_key(key) {
            return Err(MetadataError::ReservedKey {
                key: key.to_string(),
            });
        }
        let removed = self.note.meta.extra.remove(key).is_some();
        if removed {
            self.dirty_meta = true;
            self.mutated = true;
        }
        Ok(removed)
    }

    pub fn clear_extra(&mut self) -> bool {
        if self.note.meta.extra.is_empty() {
            return false;
        }
        self.note.meta.extra.clear();
        self.dirty_meta = true;
        self.mutated = true;
        true
    }

    pub fn set_extra(&mut self, extra: toml::Table) -> Result<bool, MetadataError> {
        for key in extra.keys() {
            if is_reserved_key(key) {
                return Err(MetadataError::ReservedKey { key: key.clone() });
            }
        }
        if self.note.meta.extra == extra {
            return Ok(false);
        }
        self.note.meta.extra = extra;
        self.dirty_meta = true;
        self.mutated = true;
        Ok(true)
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
    /// Arbitrary, user-defined metadata stored at the top level.
    #[serde(flatten, default, skip_serializing_if = "toml::Table::is_empty")]
    extra: toml::Table,
}

impl Metadata {
    /// Get the note's unique identifier.
    pub fn id(&self) -> &Identifier {
        &self.id
    }

    /// Get the note's creation time.
    pub fn ctime(&self) -> &DateTime<Utc> {
        &self.ctime
    }

    /// Get the note's last modified time.
    pub fn mtime(&self) -> &DateTime<Utc> {
        &self.mtime
    }

    /// Get the note's aliases.
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    /// Get the note's arbitrary metadata.
    pub fn extra(&self) -> &toml::Table {
        &self.extra
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::tempdir;

    fn non_reserved_key() -> impl Strategy<Value = String> {
        "[a-zA-Z][a-zA-Z0-9_-]{0,15}".prop_filter("non-reserved key", |key| !is_reserved_key(key))
    }

    fn toml_value_strategy() -> impl Strategy<Value = toml::Value> {
        let leaf = prop_oneof![
            any::<bool>().prop_map(toml::Value::Boolean),
            any::<i64>().prop_map(toml::Value::Integer),
            any::<f64>()
                .prop_filter("finite float", |value| value.is_finite())
                .prop_map(toml::Value::Float),
            "[a-zA-Z0-9 _-]{0,32}".prop_map(toml::Value::String),
        ];

        leaf.prop_recursive(3, 32, 4, |inner| {
            prop_oneof![
                proptest::collection::vec(inner.clone(), 0..5).prop_map(toml::Value::Array),
                proptest::collection::btree_map(non_reserved_key(), inner, 0..5).prop_map(|map| {
                    let mut table = toml::Table::new();
                    for (key, value) in map {
                        table.insert(key, value);
                    }
                    toml::Value::Table(table)
                }),
            ]
        })
    }

    fn extra_table_strategy() -> impl Strategy<Value = toml::Table> {
        proptest::collection::btree_map(non_reserved_key(), toml_value_strategy(), 0..8).prop_map(
            |map| {
                let mut table = toml::Table::new();
                for (key, value) in map {
                    table.insert(key, value);
                }
                table
            },
        )
    }

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

            let note = Note::new(&root, body.clone(), aliases.clone()).unwrap();
            let reread = Note::read(note.meta.id.clone(), root).unwrap();

            prop_assert_eq!(reread.body, body);
            prop_assert_eq!(reread.meta.aliases, aliases);
        }

        #[test]
        fn metadata_round_trips_with_extra(
            aliases in proptest::collection::vec(
                proptest::collection::vec(any::<char>(), 0..64)
                    .prop_map(|chars| chars.into_iter().collect::<String>()),
                0..8,
            ),
            extra in extra_table_strategy(),
        ) {
            let id = Identifier::new();
            let ctime = Utc::now();
            let mtime = ctime;
            let meta = Metadata {
                id: id.clone(),
                ctime,
                mtime,
                aliases: aliases.clone(),
                extra: extra.clone(),
            };
            let serialised = toml::to_string(&meta).unwrap();
            let parsed: Metadata = toml::from_str(&serialised).unwrap();

            prop_assert_eq!(parsed.id.to_string(), id.to_string());
            prop_assert_eq!(parsed.ctime, ctime);
            prop_assert_eq!(parsed.mtime, mtime);
            prop_assert_eq!(parsed.aliases, aliases);
            prop_assert_eq!(parsed.extra, extra);
        }

        #[test]
        fn note_read_preserves_extra(
            body in proptest::collection::vec(any::<char>(), 0..256)
                .prop_map(|chars| chars.into_iter().collect::<String>()),
            aliases in proptest::collection::vec(
                proptest::collection::vec(any::<char>(), 0..64)
                    .prop_map(|chars| chars.into_iter().collect::<String>()),
                0..8,
            ),
            extra in extra_table_strategy(),
        ) {
            let dir = tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let id = Identifier::new();
            let dir_path = root.join(id.to_string());
            fs::create_dir_all(&dir_path).unwrap();
            let ctime = Utc::now();
            let mtime = ctime;
            let meta = Metadata {
                id: id.clone(),
                ctime,
                mtime,
                aliases: aliases.clone(),
                extra: extra.clone(),
            };
            let meta_serialised = toml::to_string(&meta).unwrap();
            fs::write(dir_path.join("meta.toml"), meta_serialised).unwrap();
            fs::write(dir_path.join("body.md"), body.clone()).unwrap();

            let reread = Note::read(id, root).unwrap();

            prop_assert_eq!(reread.body, body);
            prop_assert_eq!(reread.meta.aliases, aliases);
            prop_assert_eq!(reread.meta.extra, extra);
        }
    }
}
