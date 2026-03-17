use std::{collections::HashMap, path::PathBuf};

use thiserror::Error;

use crate::{
    identifier::Identifier,
    note::{is_reserved_key, MetadataError, Note, NoteEditor, NoteError},
    store::{NoteStore, Store},
};

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error(transparent)]
    Note(#[from] NoteError),
    #[error(transparent)]
    Metadata(#[from] MetadataError),
    #[error("{0}")]
    Validation(String),
}

fn validation(message: impl Into<String>) -> RepositoryError {
    RepositoryError::Validation(message.into())
}

#[derive(Debug, Clone)]
pub struct CreateSpec {
    pub body: String,
    pub aliases: Vec<String>,
    pub extra: toml::Table,
}

impl CreateSpec {
    pub fn new(body: String, aliases: Vec<String>) -> Self {
        Self {
            body,
            aliases,
            extra: toml::Table::new(),
        }
    }

    fn validate(&self) -> Result<(), RepositoryError> {
        for key in self.extra.keys() {
            validate_extra_key(key)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum BodyEdit {
    Keep,
    Replace(String),
    Clear,
}

#[derive(Debug, Clone)]
pub enum AliasesEdit {
    Keep,
    Replace(Vec<String>),
    Clear,
}

#[derive(Debug, Clone)]
pub struct ExtraEdit {
    pub set: Vec<(String, toml::Value)>,
    pub unset: Vec<String>,
    pub clear: bool,
}

impl ExtraEdit {
    pub fn empty() -> Self {
        Self {
            set: Vec::new(),
            unset: Vec::new(),
            clear: false,
        }
    }

    pub fn from_raw(
        set: Vec<String>,
        unset: Vec<String>,
        clear: bool,
    ) -> Result<Self, RepositoryError> {
        let set_pairs = parse_set_pairs(&set)?;
        let mut unset_keys = Vec::with_capacity(unset.len());
        for key in unset {
            let key = key.trim();
            validate_extra_key(key)?;
            unset_keys.push(key.to_string());
        }

        let edit = Self {
            set: set_pairs,
            unset: unset_keys,
            clear,
        };
        edit.validate()?;
        Ok(edit)
    }

    fn validate(&self) -> Result<(), RepositoryError> {
        if self.clear && (!self.set.is_empty() || !self.unset.is_empty()) {
            return Err(validation(
                "--clear-extra cannot be combined with --set or --unset",
            ));
        }
        for (key, _) in &self.set {
            validate_extra_key(key)?;
        }
        for key in &self.unset {
            validate_extra_key(key)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct EditSpec {
    pub body: BodyEdit,
    pub aliases: AliasesEdit,
    pub extra: ExtraEdit,
}

impl EditSpec {
    pub fn new(body: BodyEdit, aliases: AliasesEdit, extra: ExtraEdit) -> Self {
        Self {
            body,
            aliases,
            extra,
        }
    }

    fn validate(&self) -> Result<(), RepositoryError> {
        self.extra.validate()
    }
}

#[derive(Debug)]
pub struct Repository {
    inner: RepositoryImpl<NoteStore>,
}

impl Repository {
    pub fn open(root: PathBuf) -> Self {
        Self {
            inner: RepositoryImpl::new(NoteStore::new(root)),
        }
    }

    pub fn id_for_alias(&self, alias: &str) -> Option<&Identifier> {
        self.inner.id_for_alias(alias)
    }

    pub fn create_note(&mut self, spec: CreateSpec) -> Result<Note, RepositoryError> {
        self.inner.create_note(spec)
    }

    pub fn read_note(&mut self, id: Identifier) -> Result<Note, RepositoryError> {
        self.inner.read_note(id)
    }

    pub fn edit_note(&mut self, id: Identifier, spec: EditSpec) -> Result<Note, RepositoryError> {
        self.inner.edit_note(id, spec)
    }

    pub fn edit_loaded_note(
        &mut self,
        note: &mut Note,
        spec: EditSpec,
    ) -> Result<bool, RepositoryError> {
        self.inner.edit_loaded_note(note, spec)
    }
}

#[derive(Debug)]
pub(crate) struct RepositoryImpl<S: Store> {
    store: S,
    alias_index: HashMap<String, Identifier>,
}

impl<S: Store> RepositoryImpl<S> {
    pub(crate) fn new(store: S) -> Self {
        Self {
            store,
            alias_index: HashMap::new(),
        }
    }

    pub(crate) fn id_for_alias(&self, alias: &str) -> Option<&Identifier> {
        self.alias_index.get(alias)
    }

    pub(crate) fn create_note(&mut self, spec: CreateSpec) -> Result<Note, RepositoryError> {
        spec.validate()?;
        let mut note = self.create(spec.body, spec.aliases)?;
        if !spec.extra.is_empty() {
            let extra = spec.extra;
            let changed = self.update(&mut note, |edit| edit.set_extra(extra).map(|_| ()))?;
            if changed {
                self.write(&mut note)?;
            }
        }
        Ok(note)
    }

    pub(crate) fn read_note(&mut self, id: Identifier) -> Result<Note, RepositoryError> {
        self.read(id)
    }

    pub(crate) fn edit_note(&mut self, id: Identifier, spec: EditSpec) -> Result<Note, RepositoryError> {
        let mut note = self.read(id)?;
        self.edit_loaded_note(&mut note, spec)?;
        Ok(note)
    }

    pub(crate) fn edit_loaded_note(
        &mut self,
        note: &mut Note,
        spec: EditSpec,
    ) -> Result<bool, RepositoryError> {
        spec.validate()?;
        let mutated = self.update(note, |edit| apply_edit_spec(edit, &spec))?;
        if mutated {
            self.write(note)?;
        }
        Ok(mutated)
    }

    fn create(&mut self, body: String, aliases: Vec<String>) -> Result<Note, RepositoryError> {
        let note = self.store.create(body, aliases)?;
        self.index_note(&note);
        Ok(note)
    }

    fn read(&mut self, id: Identifier) -> Result<Note, RepositoryError> {
        let note = self.store.read(id)?;
        self.index_note(&note);
        Ok(note)
    }

    fn write(&self, note: &mut Note) -> Result<(), RepositoryError> {
        self.store.write(note)?;
        Ok(())
    }

    fn update<F>(&mut self, note: &mut Note, f: F) -> Result<bool, RepositoryError>
    where
        F: FnOnce(&mut NoteEditor<'_>) -> Result<(), MetadataError>,
    {
        let old_aliases = note.meta().aliases().to_vec();
        let id = note.meta().id().clone();
        let mutated = self.store.update(note, f)?;

        if mutated && old_aliases != note.meta().aliases() {
            self.remove_aliases(&old_aliases, &id);
            self.index_note(note);
        }

        Ok(mutated)
    }

    fn index_note(&mut self, note: &Note) {
        let id = note.meta().id().clone();
        for alias in note.meta().aliases() {
            self.alias_index.insert(alias.clone(), id.clone());
        }
    }

    fn remove_aliases(&mut self, aliases: &[String], id: &Identifier) {
        for alias in aliases {
            if let Some(existing) = self.alias_index.get(alias) {
                if existing == id {
                    self.alias_index.remove(alias);
                }
            }
        }
    }
}

fn apply_edit_spec(editor: &mut NoteEditor<'_>, spec: &EditSpec) -> Result<(), MetadataError> {
    match &spec.body {
        BodyEdit::Keep => {}
        BodyEdit::Replace(body) => {
            editor.set_body(body.clone());
        }
        BodyEdit::Clear => {
            editor.set_body(String::new());
        }
    }

    match &spec.aliases {
        AliasesEdit::Keep => {}
        AliasesEdit::Replace(aliases) => {
            editor.set_aliases(aliases.clone());
        }
        AliasesEdit::Clear => {
            editor.set_aliases(Vec::new());
        }
    }

    if spec.extra.clear {
        editor.clear_extra();
    }

    for (key, value) in &spec.extra.set {
        editor.set_extra_value(key.clone(), value.clone())?;
    }

    for key in &spec.extra.unset {
        editor.remove_extra_key(key)?;
    }

    Ok(())
}

fn validate_extra_key(key: &str) -> Result<(), RepositoryError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(validation("Metadata key cannot be empty"));
    }
    if is_reserved_key(key) {
        return Err(validation(format!(
            "Metadata key `{key}` is reserved and cannot be set via extra"
        )));
    }
    if key.contains('.') {
        return Err(validation(format!(
            "Metadata key `{key}` is not supported: nested keys are not yet supported"
        )));
    }
    Ok(())
}

fn parse_set_pairs(items: &[String]) -> Result<Vec<(String, toml::Value)>, RepositoryError> {
    let mut pairs = Vec::with_capacity(items.len());
    for item in items {
        let (key, raw_value) = item
            .split_once('=')
            .ok_or_else(|| validation(format!(
                "Invalid --set value `{item}`; expected KEY=VALUE"
            )))?;
        let key = key.trim();
        let raw_value = raw_value.trim();
        if key.is_empty() {
            return Err(validation(format!(
                "Invalid --set value `{item}`; key is empty"
            )));
        }
        if raw_value.is_empty() {
            return Err(validation(format!(
                "Invalid --set value `{item}`; value is empty"
            )));
        }
        let snippet = format!("{key} = {raw_value}");
        let table: toml::Table = toml::from_str(&snippet).map_err(|error| {
            validation(format!(
                "Invalid TOML value in --set `{item}`: {error}. Try quoting strings."
            ))
        })?;
        let value = table.get(key).ok_or_else(|| {
            validation(format!(
                "Invalid --set value `{item}`; only simple top-level keys are supported"
            ))
        })?;
        pairs.push((key.to_string(), value.clone()));
    }
    Ok(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn repository_indexes_aliases_on_create_and_update() {
        let dir = tempdir().unwrap();
        let mut repo = Repository::open(dir.path().to_path_buf());

        let mut note = repo
            .create_note(CreateSpec::new(
                "body".to_string(),
                vec!["alpha".to_string()],
            ))
            .unwrap();
        assert!(repo.id_for_alias("alpha").is_some());

        let spec = EditSpec::new(
            BodyEdit::Keep,
            AliasesEdit::Replace(vec!["beta".to_string()]),
            ExtraEdit::empty(),
        );
        repo.edit_loaded_note(&mut note, spec).unwrap();

        assert!(repo.id_for_alias("alpha").is_none());
        assert!(repo.id_for_alias("beta").is_some());
    }
}
