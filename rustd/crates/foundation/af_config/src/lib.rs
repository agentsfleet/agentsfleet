//! Application-independent file layers produce the caller's validated settings type.

#![forbid(unsafe_code)]
#![deny(unused_crate_dependencies)]

mod error;
mod merge;

#[cfg(test)]
mod tests;

use std::path::Path;

use garde::{Unvalidated, Validate};
use serde::de::DeserializeOwned;

use error::ErrorKind;
pub use error::{Error, Result};

/// Configuration file access is injectable without changing process state.
pub trait FileSource {
    /// Reads the selected file or returns its actual access failure.
    ///
    /// # Errors
    /// Preserves file access and decoding failures for the loader.
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;
}

/// Local files used during process startup.
#[derive(Debug, Clone, Copy)]
pub struct LocalFiles;

impl FileSource for LocalFiles {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }
}

/// File selection and higher-priority TOML overlays, before typed deserialization.
pub struct ConfigBuilder<'a> {
    file: Option<&'a Path>,
    files: &'a dyn FileSource,
    overlays: Vec<toml::Value>,
}

impl std::fmt::Debug for ConfigBuilder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigBuilder")
            .field("file", &self.file)
            .finish_non_exhaustive()
    }
}

impl Default for ConfigBuilder<'_> {
    fn default() -> Self {
        Self {
            file: None,
            files: &LocalFiles,
            overlays: Vec::new(),
        }
    }
}

impl<'a> ConfigBuilder<'a> {
    /// Selects the file for this invocation.
    #[must_use]
    pub fn file(mut self, file: Option<&'a Path>) -> Self {
        self.file = file;
        self
    }

    /// Supplies file access, including an isolated in-memory source for tests.
    #[must_use]
    pub fn files(mut self, files: &'a dyn FileSource) -> Self {
        self.files = files;
        self
    }

    /// Later overlays override matching fields and preserve other table entries.
    #[must_use]
    pub fn overlay(mut self, overlay: toml::Value) -> Self {
        self.overlays.push(overlay);
        self
    }

    /// Returns the application's own settings after deserialization and validation.
    ///
    /// # Errors
    /// Preserves file, syntax, field and validation failures with their location.
    pub fn build<T>(self) -> Result<garde::Valid<T>>
    where
        T: DeserializeOwned + Validate<Context = ()>,
    {
        let path = self.file.unwrap_or_else(|| Path::new("<defaults>"));
        let mut merged = match self.file {
            Some(file) => {
                let contents =
                    self.files
                        .read_to_string(file)
                        .map_err(|source| ErrorKind::Read {
                            path: file.to_owned(),
                            source,
                        })?;
                parse(&contents, file)?
            }
            None => toml::Value::Table(toml::Table::new()),
        };
        for overlay in self.overlays {
            merge::merge(&mut merged, overlay);
        }
        let typed =
            serde_path_to_error::deserialize(merged).map_err(|source| ErrorKind::Field {
                path: path.to_owned(),
                field: source.path().to_string(),
                source: source.into_inner(),
            })?;
        Ok(Unvalidated::new(typed)
            .validate()
            .map_err(|source| ErrorKind::Validation {
                path: path.to_owned(),
                source,
            })?)
    }
}

fn parse(contents: &str, path: &Path) -> Result<toml::Value> {
    Ok(toml::from_str(contents).map_err(|source: toml::de::Error| {
        let line = source.span().map_or(1, |span| {
            contents[..span.start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1
        });
        ErrorKind::Syntax {
            path: path.to_owned(),
            line,
            source,
        }
    })?)
}
