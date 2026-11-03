# M236 implementation evidence

## Opening dependency verification

Indy approved implementation and latest stable dependencies on November 3, 2026.
The comparison baseline is `2f8c878c09e9c4048da1724385ec58746d348ed4`; opening documentation commit is `cd8245dbf`.
The measured runtime changes are only the Rig 0.46.0 manifest and lockfile upgrade, including transitive rig-http 0.46.0.

Crates.io metadata confirmed rig-core 0.46.0, reqwest 0.13.5, object_store 0.14.2 and aes-gcm 0.11.1 as latest stable.
No additional decision SDK or R2 runtime dependency has been introduced yet.

Run the repository's `make test-unit-rustd` with the pinned Rust 1.99.1 toolchain and all features selected by its existing shards.
The cloud host used two Cargo jobs, disabled debug information/incremental compilation, a child-reaping init wrapper and umask 022.

| Shard | Decisive command output |
|---|---|
| Runner | `unit: runner — 1490 passed` |
| Daemon | `unit: daemon — 139 passed` |
| Daemon libraries | `unit: daemon libraries — 3343 passed` |
| Combined Rust unit | 4972 passed; command exit 0. Datastore-dependent tests remain ignored in this lane. |

Full output remains at `/tmp/m236-rig-rust-tests.log` in this cloud environment.
This proves existing Rust unit compatibility after the dependency upgrade; it does not prove DT functionality or full repository verification.

Initial runs exposed host assumptions: orphaned process reaping and published-file permissions under umask 077.
The completed run used upstream Tini 0.19.0 as a subreaper and umask 022, without source edits or skipped tests.
The downloaded static binary matched upstream SHA-256 `c5b0666b4cb676901f90dfcb37106783c5fe2077b04590973b885950611b30ee`.

## Durable Task acceptance

DT schema, runtime APIs, provider integration, R2 recovery and UI behavior remain to implement.
Their integration, provider, storage, browser, performance and combined live-scenario criteria remain ungraded.
Existing SDK compatibility tests are not substitutes for those criteria.
