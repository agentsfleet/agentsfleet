//! Configuration is checked before serving or applying database migrations.

use super::{DEAD_DATABASE, FAILURE, MapEnv, SUCCESS, parse, parses_but_dead, run, status};

#[test]
fn test_no_subcommand_validates_the_selected_toml_file() {
    use std::io::Write as _;
    let mut file = tempfile::NamedTempFile::new().expect("temporary config");
    writeln!(file, "[runner.run]\nttl_seconds = 32").expect("write unsafe clock");
    let path = file.path().to_str().expect("UTF-8 fixture path");
    assert_eq!(
        status(&["agentsfleetd", "--config", path], &parses_but_dead()),
        FAILURE
    );
    file.as_file_mut()
        .set_len(0)
        .expect("truncate configuration");
    std::io::Seek::rewind(file.as_file_mut()).expect("rewind configuration");
    writeln!(file, "[runner.run]\nttl_seconds = 40").expect("write valid clock");
    let path = file.path().to_str().expect("UTF-8 fixture path");
    assert_eq!(
        status(&["agentsfleetd", "--config", path], &parses_but_dead()),
        SUCCESS
    );
}

#[test]
fn test_a_missing_explicit_config_is_a_startup_refusal() {
    assert_eq!(
        status(
            &[
                "agentsfleetd",
                "--config",
                "/missing-config-directory/agentsfleetd.toml"
            ],
            &parses_but_dead()
        ),
        FAILURE,
    );
}

#[test]
fn migration_refuses_invalid_configuration_before_building_its_runtime() {
    use std::io::Write as _;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static STARTS: AtomicUsize = AtomicUsize::new(0);
    fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
        STARTS.fetch_add(1, Ordering::Relaxed);
        Err(std::io::Error::other("observed runtime construction"))
    }
    let environment = MapEnv::from_pairs([("DATABASE_URL_MIGRATOR", DEAD_DATABASE)]);
    for contents in [
        "[runner.run]\nttl_seconds = 32",
        "[runner.stream.control]\nmax_buffered_bytes = 1",
        "[server",
    ] {
        let mut file = tempfile::NamedTempFile::new().expect("temporary config");
        writeln!(file, "{contents}").expect("invalid configuration");
        let path = file.path().to_str().expect("UTF-8 fixture path");
        assert_eq!(
            run(
                &parse(&["agentsfleetd", "--config", path, "migrate"]),
                &environment,
                runtime,
                std::future::ready(()),
            ),
            FAILURE
        );
        assert_eq!(STARTS.load(Ordering::Relaxed), 0);
    }
    let valid = tempfile::NamedTempFile::new().expect("valid empty config");
    let path = valid.path().to_str().expect("UTF-8 fixture path");
    assert_eq!(
        run(
            &parse(&["agentsfleetd", "--config", path, "migrate"]),
            &environment,
            runtime,
            std::future::ready(()),
        ),
        FAILURE
    );
    assert_eq!(STARTS.load(Ordering::Relaxed), 1);
}
