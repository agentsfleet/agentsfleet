//! Dimension 7.4 — every runtime `.sql` file `PREPARE`s against the rebuilt
//! schema, so a query naming a dropped table or column fails here, before
//! merge, rather than on the first request that runs it.
//!
//! The files are found on disk rather than listed: a `.sql` file under any
//! crate's `src/` is a statement some use case loads with `include_str!`, and a
//! list would let a new one go unchecked. The few statements still joined with
//! `const_format::concatcp!` have no file of their own, so [`JOINED`] names
//! them.

use std::path::{Path, PathBuf};

use sqlx::{AssertSqlSafe, Executor as _, SqlSafeStr as _};

use super::*;

/// The statements joined from parts with `const_format::concatcp!`, by path.
///
/// `afd_fleet`'s lease read puts each locking clause between a shared head and
/// tail so the three variants cannot drift; the head alone is no statement, so
/// only the joined whole can be prepared. Enumerate with
/// `git grep -n 'concatcp!' -- rustd/crates/*/src` and keep the SQL ones here.
const JOINED: [(&str, &str); 3] = [
    (
        "afd_fleet::lease::sql::standing::SELECT_STANDING",
        afd_fleet::lease::sql::standing::SELECT_STANDING,
    ),
    (
        "afd_fleet::lease::sql::standing::SELECT_STANDING_SHARED",
        afd_fleet::lease::sql::standing::SELECT_STANDING_SHARED,
    ),
    (
        "afd_fleet::lease::sql::tool_detail::SELECT_LIVE_LEASE",
        afd_fleet::lease::sql::tool_detail::SELECT_LIVE_LEASE,
    ),
];

/// A statement naming a column the rebuilt schema dropped.
const DROPPED_COLUMN: (&str, &str) = (
    "seeded/dropped_column.sql",
    "SELECT deletion_generation FROM authoring.drafts WHERE id = $1::uuid",
);

/// `name: error` for every statement in `statements` that does not prepare.
async fn refusals(
    connection: &mut sqlx::PgConnection,
    statements: impl IntoIterator<Item = (String, String)>,
) -> Vec<String> {
    let mut refused = Vec::new();
    for (name, sql) in statements {
        if let Err(error) = connection.prepare(AssertSqlSafe(sql).into_sql_str()).await {
            refused.push(format!("{name}: {error}"));
        }
    }
    refused
}

/// Every crate in the workspace sits beside this one.
fn crates_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("afd_db sits inside crates/")
        .to_path_buf()
}

/// Every `.sql` file below `dir`, in a stable order.
fn sql_files(dir: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            sql_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "sql") {
            found.push(path);
        }
    }
}

/// The runtime statements: `.sql` files under each crate's `src/`.
fn runtime_statements() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut crates: Vec<PathBuf> = std::fs::read_dir(crates_root())
        .unwrap()
        .map(|entry| entry.unwrap().path().join("src"))
        .filter(|src| src.is_dir())
        .collect();
    crates.sort();
    for src in crates {
        sql_files(&src, &mut found);
    }
    found
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_every_sql_file_prepares() {
    let files = runtime_statements();
    assert!(
        !files.is_empty(),
        "no .sql file under any crate's src/; the walk is broken"
    );

    let (database, db) = migrated().await;
    let mut connection = db.acquire().await.unwrap();
    let on_disk = files.iter().map(|file| {
        let name = file.strip_prefix(crates_root()).unwrap_or(file);
        (
            name.display().to_string(),
            std::fs::read_to_string(file).unwrap(),
        )
    });
    let joined = JOINED
        .iter()
        .map(|(name, sql)| ((*name).to_owned(), (*sql).to_owned()));
    let total = files.len() + JOINED.len();
    let refused = refusals(&mut connection, on_disk.chain(joined)).await;
    assert!(
        refused.is_empty(),
        "{} of {total} statements do not prepare:\n{}",
        refused.len(),
        refused.join("\n")
    );

    // The walk is only worth its green if it can go red: a statement naming a
    // dropped column is refused, and the refusal names it.
    let (name, sql) = DROPPED_COLUMN;
    let seeded = refusals(&mut connection, [(name.to_owned(), sql.to_owned())]).await;
    let named: Vec<&str> = seeded
        .iter()
        .filter_map(|line| line.split_once(": ").map(|(named, _)| named))
        .collect();
    assert_eq!(named, [name], "the refusal names the statement");
    assert!(
        seeded
            .iter()
            .all(|line| line.contains(r#"column "deletion_generation" does not exist"#)),
        "{seeded:?}"
    );

    drop(connection);
    db.close().await;
    database.cleanup().await;
}
