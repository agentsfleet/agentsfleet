# Zig test architecture

Date: Jul 26, 2026
Status: Canonical component ownership and verification topology.

---

## Component ownership

Each Zig component owns its build graph and test roots. Repository Make targets
compose those graphs. They do not rebuild a component's import list.

| Component | Unit root | Integration root | Build graph |
|---|---|---|---|
| `agentsfleetd` | `src/agentsfleetd/tests.zig` | `src/agentsfleetd/integration_tests.zig` | `build.zig` |
| `agentsfleet-runner` | `src/runner/tests.zig` | `src/runner/integration_tests.zig` | `build_runner.zig` |
| shared libraries | `src/lib/tests.zig` plus named-module roots | none | `build.zig` |

The daemon unit root imports production modules and isolated test files. The
daemon integration root imports live PostgreSQL, Redis, and QStash test files.
This separation prevents live configuration from rerunning unrelated unit tests.

The runner remains a separate build graph because it holds no datastore
credentials. Its Linux kernel tests remain separate from daemon integration.

## Public lanes

`make test-unit-agentsfleetd` runs only the daemon unit root.
`make test-integration` prepares isolated services once and runs the daemon
integration root. Component selectors reuse the same root with narrower
environment configuration.

`make memleak` builds and leak-gates the daemon, runner, and shared-library unit
binaries concurrently. The Zig allocator is blocking on macOS. The `leaks`
advisory reruns binaries only when one preflight proves that the host can inspect
child processes. Valgrind is blocking on Linux. The live boot-to-drain proof runs
after the component lanes converge.

## Coverage

`make test-coverage-zig` installs and runs five binaries under kcov:

- daemon unit tests;
- runner unit tests;
- shared library tests;
- logging tests;
- call-deadline tests.

Each binary must produce a non-empty Cobertura report. kcov merges the component
reports into `coverage/zig/merged`, and the merged line rate must meet
`ZIG_COVERAGE_MIN_LINES`. The initial floor is 60% against a measured 61.40%
baseline. Raise the floor as production-path tests land.

## Adding a component

A new Zig component adds its own unit and integration roots to its build graph.
It then adds one row to unit, integration, coverage, and memory verification
where those lanes apply. The component does not add imports to another
component's root.

The reachability gate lists compiler-registered tests across every root. Its
aggregate counts must not fall below the baseline recorded by the active
workstream.
