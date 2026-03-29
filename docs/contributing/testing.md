# Testing

## Unit tests

```bash
make test
```

Runs all unit tests. No external services required. These tests cover pure logic: parsers, state machines, serialization, and utility functions.

## Integration tests

```bash
make test-integration
```

Requires Postgres and Redis running locally (start them with `make up`). Integration tests exercise the full request path: HTTP handler to database to queue and back.

## Memory leak tests

On Linux, run the Valgrind-based memory leak detector:

```bash
make test-memleak
```

This compiles a debug build and runs the test suite under Valgrind. Any leaked allocation fails the test.

## Cross-compilation check

Verify that the build succeeds for all target platforms:

```bash
make build-cross
```

This runs the Zig cross-compilation matrix (Linux x86_64, Linux aarch64, macOS x86_64, macOS aarch64).

## Code conventions

### Module size

Every module must stay under **500 lines**. If a module grows past this limit, split it. Smaller modules are easier to test and review.

### Zig database conventions

When writing or reviewing Zig code that interacts with the database:

- **Always call `.drain()` before `.deinit()`** on `conn.query()` results. This prevents connection pool corruption.
- **Prefer `conn.exec()`** when no result rows are needed. It handles cleanup automatically.

```zig
// Good: drain before deinit
var result = try conn.query("SELECT id FROM runs WHERE status = $1", .{"QUEUED"});
defer result.deinit();
// ... process rows ...
result.drain();

// Better when no rows needed: use exec
try conn.exec("UPDATE runs SET status = $1 WHERE id = $2", .{ "RUNNING", run_id });
```

Run `make check-pg-drain` to verify all query call sites follow this convention.
