//! Dimension 7.4 — every runtime `.sql` file `PREPARE`s against the rebuilt
//! schema, so a query naming a dropped table or column fails here, before
//! merge, rather than on the first request that runs it.
//!
//! The files are found on disk rather than listed: a `.sql` file under any
//! crate's `src/` is a statement some use case loads with `include_str!`, and a
//! list would let a new one go unchecked.

use std::path::{Path, PathBuf};

use sqlx::{AssertSqlSafe, Executor as _, SqlSafeStr as _};

use super::*;

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
    let mut refused = Vec::new();
    for file in &files {
        let sql = std::fs::read_to_string(file).unwrap();
        if let Err(error) = connection.prepare(AssertSqlSafe(sql).into_sql_str()).await {
            let name = file.strip_prefix(crates_root()).unwrap_or(file);
            refused.push(format!("{}: {error}", name.display()));
        }
    }
    assert!(
        refused.is_empty(),
        "{} of {} statements do not prepare:\n{}",
        refused.len(),
        files.len(),
        refused.join("\n")
    );

    drop(connection);
    db.close().await;
    database.cleanup().await;
}
