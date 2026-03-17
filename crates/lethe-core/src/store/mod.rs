use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};

use crate::{
    identifier::Identifier,
    note::{Metadata, MetadataError, Note, NoteEditor, NoteError},
};

pub trait Clock {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub trait NoteIO {
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn write(&self, path: &Path, contents: &str) -> io::Result<()>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StdNoteIO;

impl NoteIO for StdNoteIO {
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
    }

    fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        fs::write(path, contents)
    }
}

pub trait Store {
    fn create(&self, body: String, aliases: Vec<String>) -> Result<Note, NoteError>;
    fn read(&self, id: Identifier) -> Result<Note, NoteError>;
    fn write(&self, note: &mut Note) -> Result<(), NoteError>;
    fn update<F>(
        &self,
        note: &mut Note,
        f: F,
    ) -> Result<bool, MetadataError>
    where
        F: FnOnce(&mut NoteEditor<'_>) -> Result<(), MetadataError>;
}

#[derive(Debug, Clone)]
pub struct NoteStore<I = StdNoteIO, C = SystemClock> {
    root: PathBuf,
    io: I,
    clock: C,
}

impl NoteStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            io: StdNoteIO,
            clock: SystemClock,
        }
    }
}

#[allow(dead_code)]
impl<I, C> NoteStore<I, C> {
    pub fn with_io_and_clock(root: PathBuf, io: I, clock: C) -> Self {
        Self { root, io, clock }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl<I: NoteIO, C: Clock> Store for NoteStore<I, C> {
    fn create(&self, body: String, aliases: Vec<String>) -> Result<Note, NoteError> {
        let id = Identifier::new();
        let ctime = self.clock.now();
        let mtime = ctime;
        let meta = Metadata::new(id.clone(), ctime, mtime, aliases, toml::Table::new());

        let mut note = Note::from_parts(meta, body, true, true);
        self.write(&mut note)?;

        Ok(note)
    }

    fn read(&self, id: Identifier) -> Result<Note, NoteError> {
        let dir = self.root.join(id.to_string());
        let meta_path = dir.join("meta.toml");
        let body_path = dir.join("body.md");

        let meta = match self.io.read_to_string(&meta_path) {
            Ok(val) => match toml::from_str::<Metadata>(&val) {
                Ok(val) => val,
                Err(error) => return Err(NoteError::MetadataParseError { id, error }),
            },
            Err(error) => return Err(NoteError::MetadataReadError { id, error }),
        };

        let body = self
            .io
            .read_to_string(&body_path)
            .map_err(|error| NoteError::BodyReadError { id, error })?;

        Ok(Note::from_parts(meta, body, false, false))
    }

    fn write(&self, note: &mut Note) -> Result<(), NoteError> {
        if !note.is_dirty_body() && !note.is_dirty_meta() {
            return Ok(());
        }

        let dir_path = self.root.join(note.meta().id().to_string());
        self.io
            .create_dir_all(&dir_path)
            .map_err(|error| NoteError::NoteCreateDirError {
                id: note.meta().id().clone(),
                error,
            })?;

        if note.is_dirty_meta() {
            let meta_serialised =
                toml::to_string(note.meta()).map_err(|error| NoteError::MetadataSerialiseError {
                    id: note.meta().id().clone(),
                    error,
                })?;
            let meta_path = dir_path.join("meta.toml");

            self.io
                .write(&meta_path, &meta_serialised)
                .map_err(|error| NoteError::MetadataWriteError {
                    id: note.meta().id().clone(),
                    error,
                })?;
            note.mark_meta_clean();
        }

        if note.is_dirty_body() {
            let body_path = dir_path.join("body.md");
            self.io
                .write(&body_path, note.body())
                .map_err(|error| NoteError::BodyWriteError {
                    id: note.meta().id().clone(),
                    error,
                })?;
            note.mark_body_clean();
        }

        Ok(())
    }

    fn update<F>(
        &self,
        note: &mut Note,
        f: F,
    ) -> Result<bool, MetadataError>
    where
        F: FnOnce(&mut NoteEditor<'_>) -> Result<(), MetadataError>,
    {
        let mut editor = NoteEditor::new(note);
        f(&mut editor)?;

        let mutated = editor.mutated();
        let dirty_body = editor.dirty_body();
        if mutated {
            if dirty_body {
                note.mark_dirty_body();
            }
            note.touch_at(self.clock.now());
        }

        Ok(mutated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Metadata;
    use chrono::TimeZone;
    use std::{
        cell::RefCell,
        collections::HashMap,
        rc::Rc,
        str::FromStr,
    };

    #[derive(Clone, Default)]
    struct TestIO {
        state: Rc<RefCell<IoState>>,
    }

    #[derive(Default)]
    struct IoState {
        files: HashMap<PathBuf, String>,
        fail_create_dir: Option<io::ErrorKind>,
        fail_read_meta: Option<io::ErrorKind>,
        fail_read_body: Option<io::ErrorKind>,
        fail_write_meta: Option<io::ErrorKind>,
        fail_write_body: Option<io::ErrorKind>,
        create_calls: usize,
        read_calls: usize,
        write_calls: usize,
    }

    impl TestIO {
        fn set_fail_create_dir(&self, kind: io::ErrorKind) {
            self.state.borrow_mut().fail_create_dir = Some(kind);
        }

        fn set_fail_read_meta(&self, kind: io::ErrorKind) {
            self.state.borrow_mut().fail_read_meta = Some(kind);
        }

        fn set_fail_read_body(&self, kind: io::ErrorKind) {
            self.state.borrow_mut().fail_read_body = Some(kind);
        }

        fn set_fail_write_meta(&self, kind: io::ErrorKind) {
            self.state.borrow_mut().fail_write_meta = Some(kind);
        }

        fn set_fail_write_body(&self, kind: io::ErrorKind) {
            self.state.borrow_mut().fail_write_body = Some(kind);
        }

        fn insert_file(&self, path: PathBuf, contents: impl Into<String>) {
            self.state.borrow_mut().files.insert(path, contents.into());
        }

        fn get_file(&self, path: &Path) -> Option<String> {
            self.state.borrow().files.get(path).cloned()
        }

        fn write_calls(&self) -> usize {
            self.state.borrow().write_calls
        }

        fn create_calls(&self) -> usize {
            self.state.borrow().create_calls
        }
    }

    impl NoteIO for TestIO {
        fn create_dir_all(&self, _path: &Path) -> io::Result<()> {
            let mut state = self.state.borrow_mut();
            state.create_calls += 1;
            if let Some(kind) = state.fail_create_dir {
                return Err(io::Error::new(kind, "forced create_dir_all failure"));
            }
            Ok(())
        }

        fn read_to_string(&self, path: &Path) -> io::Result<String> {
            let mut state = self.state.borrow_mut();
            state.read_calls += 1;
            if file_name_is(path, "meta.toml") {
                if let Some(kind) = state.fail_read_meta {
                    return Err(io::Error::new(kind, "forced meta read failure"));
                }
            }
            if file_name_is(path, "body.md") {
                if let Some(kind) = state.fail_read_body {
                    return Err(io::Error::new(kind, "forced body read failure"));
                }
            }
            state
                .files
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing file"))
        }

        fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
            let mut state = self.state.borrow_mut();
            state.write_calls += 1;
            if file_name_is(path, "meta.toml") {
                if let Some(kind) = state.fail_write_meta {
                    return Err(io::Error::new(kind, "forced meta write failure"));
                }
            }
            if file_name_is(path, "body.md") {
                if let Some(kind) = state.fail_write_body {
                    return Err(io::Error::new(kind, "forced body write failure"));
                }
            }
            state.files.insert(path.to_path_buf(), contents.to_string());
            Ok(())
        }
    }

    #[derive(Clone)]
    struct TestClock {
        now: Rc<RefCell<DateTime<Utc>>>,
    }

    impl TestClock {
        fn new(now: DateTime<Utc>) -> Self {
            Self {
                now: Rc::new(RefCell::new(now)),
            }
        }

        fn set(&self, now: DateTime<Utc>) {
            *self.now.borrow_mut() = now;
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> DateTime<Utc> {
            self.now.borrow().clone()
        }
    }

    fn file_name_is(path: &Path, name: &str) -> bool {
        path.file_name().and_then(|s| s.to_str()) == Some(name)
    }

    fn fixed_time(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).unwrap()
    }

    fn make_meta(id: Identifier, time: DateTime<Utc>) -> Metadata {
        Metadata::new(id, time, time, Vec::new(), toml::Table::new())
    }

    #[test]
    /// Create uses the injected clock and persists with clean dirty flags.
    fn create_uses_clock_and_cleans_dirty_flags() {
        let root = PathBuf::from("vault");
        let io = TestIO::default();
        let clock = TestClock::new(fixed_time(10));
        let store = NoteStore::with_io_and_clock(root.clone(), io.clone(), clock);

        let note = store.create("body".to_string(), vec!["a".to_string()]).unwrap();

        assert_eq!(note.meta().ctime(), &fixed_time(10));
        assert_eq!(note.meta().mtime(), &fixed_time(10));
        assert!(!note.is_dirty_body());
        assert!(!note.is_dirty_meta());

        let dir = root.join(note.meta().id().to_string());
        let meta_path = dir.join("meta.toml");
        let body_path = dir.join("body.md");
        assert!(io.get_file(&meta_path).is_some());
        assert_eq!(io.get_file(&body_path), Some("body".to_string()));
    }

    #[test]
    /// Update applies the injected clock and marks dirty metadata/body.
    fn update_uses_clock_and_marks_dirty() {
        let root = PathBuf::from("vault");
        let io = TestIO::default();
        let clock = TestClock::new(fixed_time(10));
        let store = NoteStore::with_io_and_clock(root.clone(), io, clock.clone());

        let mut note = store.create("body".to_string(), Vec::new()).unwrap();
        clock.set(fixed_time(20));

        let changed = store
            .update(&mut note, |edit| {
                edit.set_body("new body".to_string());
                Ok(())
            })
            .unwrap();

        assert!(changed);
        assert_eq!(note.meta().mtime(), &fixed_time(20));
        assert!(note.is_dirty_body());
        assert!(note.is_dirty_meta());
    }

    #[test]
    /// Write is a no-op when the note is already clean.
    fn write_skips_when_clean() {
        let root = PathBuf::from("vault");
        let io = TestIO::default();
        io.set_fail_write_meta(io::ErrorKind::PermissionDenied);
        let clock = TestClock::new(fixed_time(10));
        let store = NoteStore::with_io_and_clock(root, io.clone(), clock);

        let id = Identifier::from_str("00000000-0000-0000-0000-000000000000").unwrap();
        let meta = make_meta(id, fixed_time(10));
        let mut note = Note::from_parts(meta, "body".to_string(), false, false);

        let result = store.write(&mut note);
        assert!(result.is_ok());
        assert_eq!(io.create_calls(), 0);
        assert_eq!(io.write_calls(), 0);
    }

    #[test]
    /// Create-dir failure maps to NoteCreateDirError without clearing dirty flags.
    fn write_maps_create_dir_error() {
        let root = PathBuf::from("vault");
        let io = TestIO::default();
        io.set_fail_create_dir(io::ErrorKind::PermissionDenied);
        let clock = TestClock::new(fixed_time(10));
        let store = NoteStore::with_io_and_clock(root, io, clock);

        let id = Identifier::from_str("00000000-0000-0000-0000-000000000001").unwrap();
        let meta = make_meta(id, fixed_time(10));
        let mut note = Note::from_parts(meta, "body".to_string(), true, true);

        let result = store.write(&mut note);
        assert!(matches!(
            result,
            Err(NoteError::NoteCreateDirError { .. })
        ));
        assert!(note.is_dirty_meta());
        assert!(note.is_dirty_body());
    }

    #[test]
    /// Meta write failure maps to MetadataWriteError and keeps meta dirty.
    fn write_maps_meta_write_error_and_keeps_meta_dirty() {
        let root = PathBuf::from("vault");
        let io = TestIO::default();
        io.set_fail_write_meta(io::ErrorKind::PermissionDenied);
        let clock = TestClock::new(fixed_time(10));
        let store = NoteStore::with_io_and_clock(root, io, clock);

        let id = Identifier::from_str("00000000-0000-0000-0000-000000000002").unwrap();
        let meta = make_meta(id, fixed_time(10));
        let mut note = Note::from_parts(meta, "body".to_string(), false, true);

        let result = store.write(&mut note);
        assert!(matches!(
            result,
            Err(NoteError::MetadataWriteError { .. })
        ));
        assert!(note.is_dirty_meta());
        assert!(!note.is_dirty_body());
    }

    #[test]
    /// Body write failure maps to BodyWriteError and keeps body dirty.
    fn write_maps_body_write_error_and_keeps_body_dirty() {
        let root = PathBuf::from("vault");
        let io = TestIO::default();
        io.set_fail_write_body(io::ErrorKind::PermissionDenied);
        let clock = TestClock::new(fixed_time(10));
        let store = NoteStore::with_io_and_clock(root, io, clock);

        let id = Identifier::from_str("00000000-0000-0000-0000-000000000003").unwrap();
        let meta = make_meta(id, fixed_time(10));
        let mut note = Note::from_parts(meta, "body".to_string(), true, true);

        let result = store.write(&mut note);
        assert!(matches!(result, Err(NoteError::BodyWriteError { .. })));
        assert!(note.is_dirty_body());
        assert!(!note.is_dirty_meta());
    }

    #[test]
    /// Meta read failure maps to MetadataReadError.
    fn read_maps_meta_read_error() {
        let root = PathBuf::from("vault");
        let io = TestIO::default();
        io.set_fail_read_meta(io::ErrorKind::NotFound);
        let clock = TestClock::new(fixed_time(10));
        let store = NoteStore::with_io_and_clock(root, io, clock);

        let id = Identifier::from_str("00000000-0000-0000-0000-000000000004").unwrap();
        let result = store.read(id);
        assert!(matches!(result, Err(NoteError::MetadataReadError { .. })));
    }

    #[test]
    /// Invalid meta TOML maps to MetadataParseError.
    fn read_maps_meta_parse_error() {
        let root = PathBuf::from("vault");
        let io = TestIO::default();
        let clock = TestClock::new(fixed_time(10));
        let store = NoteStore::with_io_and_clock(root.clone(), io.clone(), clock);

        let id = Identifier::from_str("00000000-0000-0000-0000-000000000005").unwrap();
        let dir = root.join(id.to_string());
        let meta_path = dir.join("meta.toml");
        let body_path = dir.join("body.md");
        io.insert_file(meta_path, "not valid toml");
        io.insert_file(body_path, "body");

        let result = store.read(id);
        assert!(matches!(result, Err(NoteError::MetadataParseError { .. })));
    }

    #[test]
    /// Body read failure maps to BodyReadError.
    fn read_maps_body_read_error() {
        let root = PathBuf::from("vault");
        let io = TestIO::default();
        io.set_fail_read_body(io::ErrorKind::NotFound);
        let clock = TestClock::new(fixed_time(10));
        let store = NoteStore::with_io_and_clock(root.clone(), io.clone(), clock);

        let id = Identifier::from_str("00000000-0000-0000-0000-000000000006").unwrap();
        let meta = make_meta(id.clone(), fixed_time(10));
        let dir = root.join(id.to_string());
        let meta_path = dir.join("meta.toml");
        io.insert_file(meta_path, toml::to_string(&meta).unwrap());

        let result = store.read(id);
        assert!(matches!(result, Err(NoteError::BodyReadError { .. })));
    }
}
