# Store

A store is a raw collection of notes. Concretely, it is a struct which 
implements the trait

```rs
pub trait Store {
      fn read(&self, id: &Identifier) -> Result<Note, NoteError>;
      fn write(&self, note: &mut Note) -> Result<(), NoteError>;
      fn list_ids(&self) -> Result<Vec<Identifier>, NoteError>;
      fn delete(&self, id: &Identifier) -> Result<(), NoteError>;
  }
```

