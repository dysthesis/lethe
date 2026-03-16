# Documentation for Lethe

This directory contains the documentation for the codebase of Lethe.

## Data models

The data model of Lethe centres around two structures, namely

- [notes](./notes), which are directories with the note's unique _identifier_,
  as the name, each representing a single, discrete note -- each containing a 
  `meta.toml` to store the properties of the note, a `body.md`, which contains
  the text body of the note (to be manually manipulated by the user), and
- [repositories](./repository.md), as collections of notes.

Furthermore, thare are

- [identifiers](./identifiers.md) to uniquely identify each note

## Miscellaneous

- Notes on [testing](./testing.md).
