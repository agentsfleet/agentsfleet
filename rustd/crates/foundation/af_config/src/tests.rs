//! The same loader accepts independent application types and injectable file access.

use std::{cell::Cell, path::Path};

use garde::Validate;
use serde::Deserialize;

use crate::{ConfigBuilder, FileSource};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct MemoryFile<'a> {
    contents: &'a str,
    reads: Cell<usize>,
}

impl FileSource for MemoryFile<'_> {
    fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
        self.reads.set(self.reads.get() + 1);
        Ok(self.contents.to_owned())
    }
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
struct Application {
    #[garde(dive)]
    server: Server,
}

#[derive(Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
struct Server {
    #[garde(range(min = 1))]
    port: u16,
    #[garde(skip)]
    hosts: Vec<String>,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            port: 3000,
            hosts: Vec::new(),
        }
    }
}

#[test]
fn defaults_are_the_callers_own_type() -> TestResult {
    let settings = ConfigBuilder::default().build::<Application>()?;
    assert_eq!(settings.server.port, 3000);
    Ok(())
}

#[test]
fn overlays_preserve_sibling_fields_and_replace_arrays() -> TestResult {
    let file = MemoryFile {
        contents: "[server]\nport = 3100\nhosts = ['old', 'hosts']",
        reads: Cell::new(0),
    };
    let settings = ConfigBuilder::default()
        .file(Some(Path::new("application.toml")))
        .files(&file)
        .overlay(toml::from_str("[server]\nport = 3200")?)
        .overlay(toml::from_str("[server]\nhosts = ['replacement']")?)
        .build::<Application>()?;
    assert_eq!(settings.server.port, 3200);
    assert_eq!(settings.server.hosts, ["replacement"]);
    assert_eq!(file.reads.get(), 1);
    Ok(())
}

#[test]
fn invocation_values_override_before_deserializing_the_file() -> TestResult {
    let file = MemoryFile {
        contents: "[server]\nport = 'replaced-scalar'",
        reads: Cell::new(0),
    };
    let settings = ConfigBuilder::default()
        .file(Some(Path::new("application.toml")))
        .files(&file)
        .overlay(toml::from_str("[server]\nport = 3300")?)
        .build::<Application>()?;
    assert_eq!(settings.server.port, 3300);
    Ok(())
}

#[test]
fn field_errors_identify_the_path_without_echoing_values() -> TestResult {
    for contents in [
        "[server]\nport = 'private-value'",
        "[server]\nunknown = 'private-value'",
    ] {
        let file = MemoryFile {
            contents,
            reads: Cell::new(0),
        };
        let result = ConfigBuilder::default()
            .file(Some(Path::new("application.toml")))
            .files(&file)
            .build::<Application>();
        let failure = result.err().ok_or("invalid field must refuse loading")?;
        let message = failure.to_string();
        assert!(message.contains("application.toml"));
        assert!(message.contains("server"));
        assert!(!message.contains("private-value"));
    }
    Ok(())
}

#[test]
fn syntax_and_guard_failures_keep_the_file_location() -> TestResult {
    for contents in ["[server", "[server]\nport = 0"] {
        let file = MemoryFile {
            contents,
            reads: Cell::new(0),
        };
        let result = ConfigBuilder::default()
            .file(Some(Path::new("application.toml")))
            .files(&file)
            .build::<Application>();
        let failure = result
            .err()
            .ok_or("invalid syntax or range must refuse loading")?;
        assert!(failure.to_string().contains("application.toml"));
        assert_eq!(failure.code(), af_core::error_code::STARTUP_ENV_CHECK);
    }
    Ok(())
}

#[test]
fn another_application_supplies_its_own_schema() -> TestResult {
    #[derive(Deserialize, Validate)]
    #[serde(deny_unknown_fields)]
    struct RunnerSettings {
        #[garde(range(min = 1, max = 64))]
        workers: u32,
    }
    let settings = ConfigBuilder::default()
        .overlay(toml::from_str("workers = 16")?)
        .build::<RunnerSettings>()?;
    assert_eq!(settings.workers, 16);
    Ok(())
}

#[test]
fn file_access_failure_preserves_its_io_source() -> TestResult {
    use std::error::Error as _;
    struct Missing;
    impl FileSource for Missing {
        fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        }
    }
    let result = ConfigBuilder::default()
        .file(Some(Path::new("protected.toml")))
        .files(&Missing)
        .build::<Application>();
    let failure = result.err().ok_or("file source must refuse access")?;
    let source = failure
        .source()
        .ok_or("file access source must be preserved")?;
    assert!(source.is::<std::io::Error>());
    Ok(())
}

#[test]
fn the_debug_view_names_the_selected_file_and_not_its_overlays() {
    assert_eq!(
        format!("{:?}", ConfigBuilder::default()),
        "ConfigBuilder { file: None, .. }"
    );
    let selected = ConfigBuilder::default().file(Some(Path::new("agentsfleet.toml")));
    assert_eq!(
        format!("{selected:?}"),
        r#"ConfigBuilder { file: Some("agentsfleet.toml"), .. }"#
    );
}
