# Zig test architecture

Date: Jul 26, 2026
Status: Canonical component ownership and verification topology.

---

## Component ownership

Each Zig component owns its build graph and test roots. Repository Make targets
compose those graphs. They do not rebuild a component's import list.

| Root | Lane | Build graph |
|---|---|---|
| `src/agentsfleetd/tests.zig` | daemon unit (`test`) | `build.zig` |
| `src/agentsfleetd/integration_tests.zig` | daemon integration (`test-integration`) | `build.zig` |
| `src/agentsfleetd/auth/tests.zig` | auth portability (`test-auth`) | `build.zig` |
| `src/lib/tests.zig` | shared-library barrel (`test-lib`) | `build.zig` |
| `src/lib/logging/mod.zig` | logging named module (`test-lib`) | `build.zig` |
| `src/lib/call_deadline/call_deadline.zig` | call-deadline named module (`test-lib`) | `build.zig` |
| `src/runner/tests.zig` | runner unit (`test`) | `build_runner.zig` |
| `src/runner/sandbox_integration_test.zig` | runner integration (`test-integration`) | `build_runner.zig` |

Eight roots, not three. Two of them — the logging and call-deadline entries —
are production module roots whose own `test` block doubles as the lane root, so
they carry imports the module needs rather than an aggregate list. The runner's
integration root is likewise a test file that force-imports three siblings, not
a dedicated aggregate root; there is no `src/runner/integration_tests.zig`.

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

`make test-coverage-zig` installs and runs eight component binaries under kcov:

- daemon unit tests;
- runner unit tests;
- shared library tests;
- logging tests;
- call-deadline tests;
- object-store tests;
- the runner integration suite (Linux only);
- the daemon integration suite, against live datastores, serially.

`scripts/check_zig_coverage.py` unions those reports — a line counts as covered
when any component executed it, because the unit lanes and the integration
suites reach largely disjoint code — publishes the union to
`coverage/zig/merged`, and enforces `ZIG_COVERAGE_MIN_LINES`. The floor is 89%.

The union is deliberately not `kcov --merge`. That command returned only the
three `src/lib` components on Linux — 24 files, 861 lines — against 558 files
and 31,259 lines from the identical invocation on macOS, same kcov 43. The gate
read the result without checking what it covered, so it graded 2.8% of the
codebase and reported 93.70%.

### Test binaries compile through LLVM

Zig 0.16's self-hosted x86_64 backend emits DWARF 5 line programs libdw
rejects. `dwarf_getsrclines` returns `invalid .debug_line section` for every Zig
unit; binutils reports bogus sibling markers over the same bytes. kcov reads
line tables through libdw and skips failing units silently. Six of eight
components measured nothing on Linux; `agentsfleetd/` contributed nothing at
all. Only `compiler_rt` survived — the one DWARF 4 unit per binary.

So every test binary sets `use_llvm`, from one definition site:
`shared.TEST_USE_LLVM`. Under LLVM the same sources parse whole. Measured with
real kcov: `logging` 0 → 7 product classes, `deadline` 0 → 8, matching macOS.

**Do not drop `use_llvm` from a test binary to speed up a build.** Its coverage
silently falls to zero instead of failing — a skipped unit and an untested unit
look identical in the report. `ZIG_COVERAGE_REQUIRED_COMPONENTS` is the only
alarm.

Two dead ends, so nobody retries them: kcov is current at v43 (`v44-pre-test3`
is identical), and elfutils 0.190 and 0.192 both refuse the same bytes. The
defect was in the debug info, not the readers.

So the gate grades the union of the components that did collect and states
`measured over N of M components`, naming every component that captured
nothing, on success and on failure alike. `ZIG_COVERAGE_REQUIRED_COMPONENTS`
(`make/test.mk`, one definition site per platform) names those that must
collect; a required component contributing nothing fails the build, which is
the regression the earlier blanket refusal was written to catch.

**Read the rate with its denominator.** A subset flatters. While Linux measured
only `runner` and `lib` it graded ~92% over 89 files; all seven macOS components
measured 90.26% over 565. `zig_components_measured` and `zig_measured_files` tell
them apart. A rate that rises while the file count falls is a capture regression.

`ZIG_COVERAGE_REQUIRED_COMPONENTS` ratchets on evidence. A component joins it in
the commit where a green run shows it collecting — never ahead.
Until kcov collects the remaining components, per-folder floors for
`agentsfleetd/` cannot be enforced in Continuous Integration (CI) at all,
because that tree contributes no measured line there.

The checker writes these keys to `.tmp/zig-coverage.txt` for the CI job summary:
`zig_line_coverage_pct`, `zig_line_coverage_min_pct`, `zig_measured_files`,
`zig_measured_lines`, `zig_components_measured`, `zig_components_total`, and
`zig_components_empty`.

## Adding a component

A new Zig component adds its own unit and integration roots to its build graph.
It then adds one row to unit, integration, coverage, and memory verification
where those lanes apply. The component does not add imports to another
component's root.

The reachability gate lists compiler-registered tests across every root. Its
aggregate counts must not fall below the baseline recorded by the active
workstream.
