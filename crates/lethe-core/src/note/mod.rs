use std::io;

use chrono::{DateTime, Utc};
#[cfg(all(test, feature = "arbitrary"))]
use proptest_derive::Arbitrary;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::identifier::Identifier;

const RESERVED_KEYS: [&str; 4] = ["id", "ctime", "mtime", "aliases"];

pub(crate) fn is_reserved_key(key: &str) -> bool {
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
    pub(crate) fn from_parts(
        meta: Metadata,
        body: String,
        dirty_body: bool,
        dirty_meta: bool,
    ) -> Self {
        Self {
            meta,
            body,
            dirty_body,
            dirty_meta,
        }
    }

    pub(crate) fn is_dirty_body(&self) -> bool {
        self.dirty_body
    }

    pub(crate) fn is_dirty_meta(&self) -> bool {
        self.dirty_meta
    }

    pub(crate) fn mark_body_clean(&mut self) {
        self.dirty_body = false;
    }

    pub(crate) fn mark_meta_clean(&mut self) {
        self.dirty_meta = false;
    }

    pub(crate) fn mark_dirty_body(&mut self) {
        self.dirty_body = true;
    }

    pub fn set_body(&mut self, body: String) {
        if self.body == body {
            return;
        }
        self.body = body;
        self.dirty_body = true;
        self.touch_at(Utc::now());
    }

    /// Get the note's metadata
    pub fn meta(&self) -> &Metadata {
        &self.meta
    }

    /// Get the note's body
    pub fn body(&self) -> &str {
        &self.body
    }

    pub(crate) fn touch_at(&mut self, now: DateTime<Utc>) {
        self.meta.mtime = now;
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
    pub(crate) fn new(note: &'a mut Note) -> Self {
        Self {
            note,
            mutated: false,
            dirty_body: false,
            dirty_meta: false,
        }
    }

    pub(crate) fn mutated(&self) -> bool {
        self.mutated
    }

    pub(crate) fn dirty_body(&self) -> bool {
        self.dirty_body
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
    pub(crate) fn new(
        id: Identifier,
        ctime: DateTime<Utc>,
        mtime: DateTime<Utc>,
        aliases: Vec<String>,
        extra: toml::Table,
    ) -> Self {
        Self {
            id,
            ctime,
            mtime,
            aliases,
            extra,
        }
    }

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
    use crate::store::{NoteStore, Store};
    use proptest::prelude::*;
    use std::fs;
    use tempfile::tempdir;

    fn body_strategy() -> impl Strategy<Value = String> {
        proptest::collection::vec(any::<char>(), 0..256)
            .prop_map(|chars| chars.into_iter().collect::<String>())
    }

    fn aliases_strategy() -> impl Strategy<Value = Vec<String>> {
        proptest::collection::vec(
            proptest::collection::vec(any::<char>(), 0..64)
                .prop_map(|chars| chars.into_iter().collect::<String>()),
            0..8,
        )
    }

    fn alias_string_strategy() -> impl Strategy<Value = String> {
        proptest::collection::vec(any::<char>(), 0..64)
            .prop_map(|chars| chars.into_iter().collect::<String>())
    }

    fn distinct_body_pair() -> impl Strategy<Value = (String, String)> {
        (
            body_strategy(),
            proptest::collection::vec(any::<char>(), 1..32),
        )
            .prop_map(|(body, suffix)| {
                let mut other = body.clone();
                other.extend(suffix);
                (body, other)
            })
    }

    fn distinct_aliases_pair() -> impl Strategy<Value = (Vec<String>, Vec<String>)> {
        (aliases_strategy(), alias_string_strategy()).prop_map(|(aliases, extra_alias)| {
            let mut other = aliases.clone();
            other.push(extra_alias);
            (aliases, other)
        })
    }

    fn non_reserved_key() -> impl Strategy<Value = String> {
        "[a-zA-Z][a-zA-Z0-9_-]{0,15}".prop_filter("non-reserved key", |key| !is_reserved_key(key))
    }

    fn distinct_key_pair() -> impl Strategy<Value = (String, String)> {
        proptest::collection::btree_set(non_reserved_key(), 2..=2).prop_map(|set| {
            let mut iter = set.into_iter();
            (iter.next().unwrap(), iter.next().unwrap())
        })
    }

    fn reserved_key_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("id".to_string()),
            Just("ctime".to_string()),
            Just("mtime".to_string()),
            Just("aliases".to_string()),
        ]
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

    fn non_empty_extra_strategy() -> impl Strategy<Value = toml::Table> {
        extra_table_strategy().prop_map(|mut table| {
            if table.is_empty() {
                table.insert("x".to_string(), toml::Value::Boolean(true));
            }
            table
        })
    }

    fn different_value(value: &toml::Value) -> toml::Value {
        match value {
            toml::Value::Boolean(value) => toml::Value::Boolean(!value),
            toml::Value::Integer(value) => {
                let next = if *value == i64::MAX {
                    i64::MIN
                } else {
                    value + 1
                };
                toml::Value::Integer(next)
            }
            toml::Value::Float(value) => {
                let next = if *value == 0.0 { 1.0 } else { 0.0 };
                toml::Value::Float(next)
            }
            toml::Value::String(value) => toml::Value::String(format!("{value}x")),
            toml::Value::Array(values) => {
                let mut next = values.clone();
                next.push(toml::Value::Boolean(true));
                toml::Value::Array(next)
            }
            toml::Value::Table(values) => {
                let mut next = values.clone();
                let mut key = "x".to_string();
                while next.contains_key(&key) {
                    key.push('x');
                }
                next.insert(key, toml::Value::Boolean(true));
                toml::Value::Table(next)
            }
            toml::Value::Datetime(value) => toml::Value::String(value.to_string()),
        }
    }

    fn different_extra(extra: &toml::Table) -> toml::Table {
        let mut next = extra.clone();
        if let Some((key, value)) = next.iter().next().map(|(k, v)| (k.clone(), v.clone())) {
            next.insert(key, different_value(&value));
            return next;
        }

        let mut key = "x".to_string();
        while next.contains_key(&key) {
            key.push('x');
        }
        next.insert(key, toml::Value::Boolean(true));
        next
    }

    fn distinct_extra_pair() -> impl Strategy<Value = (toml::Table, toml::Table)> {
        non_empty_extra_strategy().prop_map(|extra| {
            let other = different_extra(&extra);
            (extra, other)
        })
    }

    #[derive(Clone, Debug)]
    struct Model {
        id: Identifier,
        ctime: DateTime<Utc>,
        mtime: DateTime<Utc>,
        body: String,
        aliases: Vec<String>,
        extra: toml::Table,
        dirty_body: bool,
        dirty_meta: bool,
    }

    impl Model {
        fn from_note(note: &Note) -> Self {
            Self {
                id: note.meta.id.clone(),
                ctime: note.meta.ctime.clone(),
                mtime: note.meta.mtime.clone(),
                body: note.body.clone(),
                aliases: note.meta.aliases.clone(),
                extra: note.meta.extra.clone(),
                dirty_body: note.dirty_body,
                dirty_meta: note.dirty_meta,
            }
        }

        fn apply_op(&mut self, op: &Op) -> (bool, bool) {
            let mut changed_body = false;
            let mut changed_meta = false;
            match op {
                Op::SetBody(body) => {
                    if self.body != *body {
                        self.body = body.clone();
                        changed_body = true;
                    }
                }
                Op::SetAliases(aliases) => {
                    if self.aliases != *aliases {
                        self.aliases = aliases.clone();
                        changed_meta = true;
                    }
                }
                Op::SetExtraValue { key, value } => {
                    debug_assert!(!is_reserved_key(key));
                    let changed = match self.extra.get(key) {
                        Some(existing) => existing != value,
                        None => true,
                    };
                    if changed {
                        self.extra.insert(key.clone(), value.clone());
                        changed_meta = true;
                    }
                }
                Op::RemoveExtraKey { key } => {
                    debug_assert!(!is_reserved_key(key));
                    if self.extra.remove(key).is_some() {
                        changed_meta = true;
                    }
                }
                Op::ClearExtra => {
                    if !self.extra.is_empty() {
                        self.extra.clear();
                        changed_meta = true;
                    }
                }
                Op::SetExtra(extra) => {
                    if self.extra != *extra {
                        self.extra = extra.clone();
                        changed_meta = true;
                    }
                }
            }

            if changed_body || changed_meta {
                self.dirty_body |= changed_body;
                self.dirty_meta = true;
                self.mtime = Utc::now();
            }

            (changed_body, changed_meta)
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    struct Snapshot {
        id: String,
        ctime: DateTime<Utc>,
        mtime: DateTime<Utc>,
        body: String,
        aliases: Vec<String>,
        extra: toml::Table,
        dirty_body: bool,
        dirty_meta: bool,
    }

    fn snapshot(note: &Note) -> Snapshot {
        Snapshot {
            id: note.meta.id.to_string(),
            ctime: note.meta.ctime.clone(),
            mtime: note.meta.mtime.clone(),
            body: note.body.clone(),
            aliases: note.meta.aliases.clone(),
            extra: note.meta.extra.clone(),
            dirty_body: note.dirty_body,
            dirty_meta: note.dirty_meta,
        }
    }

    fn make_note(body: String, aliases: Vec<String>, extra: toml::Table) -> Note {
        let id = Identifier::new();
        let ctime = Utc::now();
        let mtime = ctime;
        Note {
            meta: Metadata {
                id,
                ctime,
                mtime,
                aliases,
                extra,
            },
            body,
            dirty_body: false,
            dirty_meta: false,
        }
    }

    #[derive(Clone, Debug)]
    enum Op {
        SetBody(String),
        SetAliases(Vec<String>),
        SetExtraValue { key: String, value: toml::Value },
        RemoveExtraKey { key: String },
        ClearExtra,
        SetExtra(toml::Table),
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            body_strategy().prop_map(Op::SetBody),
            aliases_strategy().prop_map(Op::SetAliases),
            (non_reserved_key(), toml_value_strategy())
                .prop_map(|(key, value)| { Op::SetExtraValue { key, value } }),
            non_reserved_key().prop_map(|key| Op::RemoveExtraKey { key }),
            Just(Op::ClearExtra),
            extra_table_strategy().prop_map(Op::SetExtra),
        ]
    }

    fn apply_op_editor(editor: &mut NoteEditor<'_>, op: &Op) -> Result<(), MetadataError> {
        let _ = editor.body();
        let _ = editor.meta();
        match op {
            Op::SetBody(body) => {
                editor.set_body(body.clone());
                Ok(())
            }
            Op::SetAliases(aliases) => {
                editor.set_aliases(aliases.clone());
                Ok(())
            }
            Op::SetExtraValue { key, value } => {
                editor.set_extra_value(key.clone(), value.clone())?;
                Ok(())
            }
            Op::RemoveExtraKey { key } => {
                editor.remove_extra_key(key)?;
                Ok(())
            }
            Op::ClearExtra => {
                editor.clear_extra();
                Ok(())
            }
            Op::SetExtra(extra) => {
                editor.set_extra(extra.clone())?;
                Ok(())
            }
        }
    }

    proptest! {
        #[test]
        // Round-trip a note through disk I/O and verify body and aliases are
        // preserved.
        fn note_round_trips(
            body in body_strategy(),
            aliases in aliases_strategy(),
        ) {
            let dir = tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let store = NoteStore::new(root.clone());

            let note = store.create(body.clone(), aliases.clone()).unwrap();
            let reread = store.read(note.meta().id().clone()).unwrap();

            prop_assert_eq!(reread.body, body);
            prop_assert_eq!(reread.meta.aliases, aliases);
        }

        #[test]
        // Ensure metadata TOML serialisation preserves id, timestamps, aliases,
        // and extra fields.
        fn metadata_round_trips_with_extra(
            aliases in aliases_strategy(),
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
        // Reading a note should preserve extra metadata written to disk.
        fn note_read_preserves_extra(
            body in body_strategy(),
            aliases in aliases_strategy(),
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

            let store = NoteStore::new(root);
            let reread = store.read(id).unwrap();

            prop_assert_eq!(reread.body, body);
            prop_assert_eq!(reread.meta.aliases, aliases);
            prop_assert_eq!(reread.meta.extra, extra);
        }

        #[test]
        // Apply a random edit sequence and assert Note matches a pure model
        // after each step.
        fn note_editor_sequence_matches_model(
            body in body_strategy(),
            aliases in aliases_strategy(),
            extra in extra_table_strategy(),
            ops in proptest::collection::vec(op_strategy(), 0..32),
        ) {
            let mut note = make_note(body, aliases, extra);
            let mut model = Model::from_note(&note);
            let store = NoteStore::new(std::path::PathBuf::new());

            for op in ops {
                let old_mtime = note.meta.mtime.clone();
                let old_dirty_body = note.dirty_body;
                let old_dirty_meta = note.dirty_meta;

                let (changed_body, changed_meta) = model.apply_op(&op);
                let expected_mutated = changed_body || changed_meta;

                let mutated =
                    store.update(&mut note, |editor| apply_op_editor(editor, &op)).unwrap();
                prop_assert_eq!(mutated, expected_mutated);

                let note_meta = note.meta();
                prop_assert_eq!(note.body(), model.body.as_str());
                prop_assert_eq!(note_meta.aliases(), model.aliases.as_slice());
                prop_assert_eq!(note_meta.extra(), &model.extra);
                prop_assert_eq!(note_meta.id().to_string(), model.id.to_string());
                prop_assert_eq!(note_meta.ctime(), &model.ctime);

                let note_mtime = note_meta.mtime().clone();
                if expected_mutated {
                    prop_assert_eq!(note.dirty_body, old_dirty_body || changed_body);
                    prop_assert!(note.dirty_meta);
                    prop_assert!(note_mtime >= old_mtime);
                } else {
                    prop_assert_eq!(note.dirty_body, old_dirty_body);
                    prop_assert_eq!(note.dirty_meta, old_dirty_meta);
                    prop_assert_eq!(note_mtime, old_mtime);
                }

                prop_assert_eq!(note.dirty_body, model.dirty_body);
                prop_assert_eq!(note.dirty_meta, model.dirty_meta);
            }
        }

        #[test]
        // Reserved keys must be rejected and leave note state unchanged.
        fn reserved_keys_rejected(
            body in body_strategy(),
            aliases in aliases_strategy(),
            extra in extra_table_strategy(),
            key in reserved_key_strategy(),
            value in toml_value_strategy(),
        ) {
            let mut note = make_note(body, aliases, extra);
            let store = NoteStore::new(std::path::PathBuf::new());

            let before = snapshot(&note);
            let res = store.update(&mut note, |editor| {
                editor.set_extra_value(key.clone(), value.clone()).map(|_| ())
            });
            prop_assert!(res.is_err());
            prop_assert_eq!(snapshot(&note), before);

            let before = snapshot(&note);
            let res = store
                .update(&mut note, |editor| editor.remove_extra_key(&key).map(|_| ()));
            prop_assert!(res.is_err());
            prop_assert_eq!(snapshot(&note), before);

            let before = snapshot(&note);
            let mut table = toml::Table::new();
            table.insert(key, value);
            let res = store.update(&mut note, |editor| editor.set_extra(table).map(|_| ()));
            prop_assert!(res.is_err());
            prop_assert_eq!(snapshot(&note), before);
        }

        #[test]
        // Force no-op vs change paths for editor mutations and validate mtime
        // behavior.
        fn note_editor_noop_and_change_branches(
            (body, other_body) in distinct_body_pair(),
            (aliases, other_aliases) in distinct_aliases_pair(),
            (key, other_key) in distinct_key_pair(),
            value in toml_value_strategy(),
            (extra, other_extra) in distinct_extra_pair(),
        ) {
            let other_value = different_value(&value);

            let mut extra_with_key = toml::Table::new();
            extra_with_key.insert(key.clone(), value.clone());

            let mut note = make_note(
                body.clone(),
                aliases.clone(),
                extra_with_key
            );
            let store = NoteStore::new(std::path::PathBuf::new());

            let old_mtime = note.meta.mtime.clone();
            let mutated = store.update(&mut note, |editor| {
                editor.set_body(body.clone());
                Ok(())
            }).unwrap();
            prop_assert!(!mutated);
            prop_assert_eq!(note.meta.mtime, old_mtime);

            let old_mtime = note.meta.mtime.clone();
            let mutated = store.update(&mut note, |editor| {
                editor.set_body(other_body.clone());
                Ok(())
            }).unwrap();
            prop_assert!(mutated);
            prop_assert!(note.meta.mtime >= old_mtime);

            let old_mtime = note.meta.mtime.clone();
            let mutated = store.update(&mut note, |editor| {
                editor.set_aliases(aliases.clone());
                Ok(())
            }).unwrap();
            prop_assert!(!mutated);
            prop_assert_eq!(note.meta.mtime, old_mtime);

            let old_mtime = note.meta.mtime.clone();
            let mutated = store.update(&mut note, |editor| {
                editor.set_aliases(other_aliases.clone());
                Ok(())
            }).unwrap();
            prop_assert!(mutated);
            prop_assert!(note.meta.mtime >= old_mtime);

            let old_mtime = note.meta.mtime.clone();
            let mutated = store.update(&mut note, |editor| {
                editor.set_extra_value(key.clone(), value.clone()).map(|_| ())
            }).unwrap();
            prop_assert!(!mutated);
            prop_assert_eq!(note.meta.mtime, old_mtime);

            let old_mtime = note.meta.mtime.clone();
            let mutated = store.update(&mut note, |editor| {
                editor.remove_extra_key(&other_key).map(|_| ())
            }).unwrap();
            prop_assert!(!mutated);
            prop_assert_eq!(note.meta.mtime, old_mtime);

            let old_mtime = note.meta.mtime.clone();
            let mutated = store.update(&mut note, |editor| {
                editor.clear_extra();
                Ok(())
            }).unwrap();
            prop_assert!(mutated);
            prop_assert!(note.meta.mtime >= old_mtime);

            let old_mtime = note.meta.mtime.clone();
            let mutated = store.update(&mut note, |editor| {
                editor.clear_extra();
                Ok(())
            }).unwrap();
            prop_assert!(!mutated);
            prop_assert_eq!(note.meta.mtime, old_mtime);

            let old_mtime = note.meta.mtime.clone();
            let mutated = store.update(&mut note, |editor| {
                editor.set_extra_value(key.clone(), other_value).map(|_| ())
            }).unwrap();
            prop_assert!(mutated);
            prop_assert!(note.meta.mtime >= old_mtime);

            let old_mtime = note.meta.mtime.clone();
            let mutated = store.update(&mut note, |editor| {
                editor.remove_extra_key(&key).map(|_| ())
            }).unwrap();
            prop_assert!(mutated);
            prop_assert!(note.meta.mtime >= old_mtime);

            let old_mtime = note.meta.mtime.clone();
            let mutated = store.update(&mut note, |editor| editor.set_extra(extra.clone())
                                                     .map(|_| ())).unwrap();
            prop_assert!(mutated);
            prop_assert!(note.meta.mtime >= old_mtime);

            let old_mtime = note.meta.mtime.clone();
            let mutated = store.update(&mut note, |editor| editor.set_extra(extra)
                                                     .map(|_| ())).unwrap();
            prop_assert!(!mutated);
            prop_assert_eq!(note.meta.mtime, old_mtime);

            let old_mtime = note.meta.mtime.clone();
            let mutated =
                store.update(&mut note, |editor| editor.set_extra(other_extra)
                                           .map(|_| ())).unwrap();
            prop_assert!(mutated);
            prop_assert!(note.meta.mtime >= old_mtime);
        }

        #[test]
        // Verify `Note::set_body` is a no-op on same input and mutates on
        // different input.
        fn note_set_body_noop_and_change(
            (body, other_body) in distinct_body_pair(),
            aliases in aliases_strategy(),
            extra in extra_table_strategy(),
        ) {
            let mut note = make_note(body.clone(), aliases, extra);
            let old_mtime = note.meta.mtime.clone();
            note.set_body(body);
            prop_assert_eq!(note.meta.mtime, old_mtime);
            prop_assert!(!note.dirty_body);
            prop_assert!(!note.dirty_meta);

            let old_mtime = note.meta.mtime.clone();
            note.set_body(other_body);
            prop_assert!(note.meta.mtime >= old_mtime);
            prop_assert!(note.dirty_body);
            prop_assert!(note.dirty_meta);
        }
    }
}
