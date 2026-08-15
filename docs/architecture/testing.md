# Zig test architecture

Canonical component ownership and verification topology.

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
a dedicated aggregate root. The runner has no integration aggregate root at all.

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

`make test-coverage-zig` installs and runs nine component binaries under kcov:

- `agentsfleetd` — daemon unit tests;
- `runner` — runner unit tests;
- `lib` — shared library tests;
- `logging` — logging tests;
- `deadline` — call-deadline tests;
- `s3` — object-store tests;
- `runner_integration` — the runner integration suite, whose worker-pool tests
  fork the real stub child on Linux and macOS alike;
- `integration` — the daemon integration suite, against live datastores, serially;
- `lifecycle` — the boot to SIGTERM to drain proof, alone in its own process
  against live datastores.

`scripts/check_zig_coverage.py` unions those reports — a line counts as covered
when any component executed it, because the unit lanes and the integration
suites reach largely disjoint code — publishes the union to
`coverage/zig/merged`, and enforces `ZIG_COVERAGE_MIN_PCT` plus one floor per
product folder.

`lifecycle` measures the only test that drives the real `serve.run`. It runs
last and costs a rebuild: the integration binary takes its test filter at build
time, and this test cannot share a process with the rest of the suite, since it
installs signal handlers, binds a port and moves process-global state the other
tests read. Before it existed, `cmd/serve.zig` — the daemon's entire boot
sequence — measured 0% of 116 lines. Both lanes that run it, this one and
`make memleak`, grep its run marker: a test that skipped still yields a valid
report, describing a process that started and stopped.

### The denominator holds shipped code only

A line enters the denominator only if it ships. Three exclusions, all in
`scripts/check_zig_coverage.py`:

- `*_test.zig` files, dropped by kcov's `--exclude-pattern`;
- test-support sources — harnesses, fixtures, fakes — matched by naming form;
- `test { ... }` blocks written inside product files.

The third one mattered most. Those blocks held 5,309 lines, 17% of the old
denominator, and a test body is ~100% covered by construction, so they lifted
every published rate by 1.7 to 2.6 points. Worse, they made the gate satisfiable
by writing more tests inside a file rather than covering more of it. Rates fell
when this landed; coverage did not regress.

### Floors bind per folder

One merged floor averages the daemon against the runner, so no floor could ever
bind the daemon — the tree that carries most of the risk. Each product folder
now has its own.

| Scope | Enforced floor | Target |
|---|---|---|
| merged | 89 | 95 |
| `agentsfleetd` | 89 | 95 |
| `runner` | 92 | 95 |
| `lib` | 93 | 95 |

Floors are enforced and **raise-only**: move one in the same commit as the tests
that measurably clear it, never ahead. A floor set ahead of its tests gates
nothing but red, which is what 91 did here once already. Targets are published
and never enforced; the gap between floor and target prints every run, so the
destination stays visible without an unmet one turning the build red.

`make/test.mk` is the single definition site for every one of these numbers, and
`scripts/check_zig_coverage_doc_test.py` fails when this table disagrees with
it. The values above were stale for exactly as long as nothing checked them.

A 95% target is reachable for the daemon and the runner but **not for `lib`**,
whose measured ceiling is 97.05% and falling: kcov attributes no instructions to
a function signature, a parameter line, a closing brace or a comment, so those
lines cannot be covered by any test. Other subsystems ceil between 99.38% and
99.69%.

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

`ZIG_COVERAGE_REQUIRED_ROOTS` is the companion assertion on the union itself:
`agentsfleetd`, `runner` and `lib` must each carry a measured line, whatever the
rate. A union at 98% holding one tree is not a measurement of the codebase.
`ZIG_COVERAGE_MIN_FILES` and `ZIG_COVERAGE_MIN_MEASURED_LINES` sit under the
whole report at roughly half the measured figures — a collapse alarm, not a
growth ratchet, set low on purpose so deleting dead code never turns the gate
red.

The checker writes these keys to `.tmp/zig-coverage.txt` for the Continuous
Integration (CI) job summary: `zig_line_coverage_pct`,
`zig_line_coverage_min_pct`, `zig_line_coverage_target_pct`,
`zig_line_coverage_gap_pts`, `zig_measured_files`, `zig_measured_lines`,
`zig_components_measured`, `zig_components_total`, `zig_components_empty`, and
one `zig_folder_pct_*`, `zig_folder_min_pct_*`, `zig_folder_target_pct_*` and
`zig_folder_gap_pts_*` per product folder.

The README badge does **not** read those keys. It reads Codecov, which is fed
`coverage/zig/merged/cobertura.xml` — the union this checker publishes, with the
denominator rules above already applied. Uploading the raw per-component kcov
reports instead would let Codecov build its own union over its own denominator,
and the badge would disagree with the gate by roughly two points. Upload the
merged report, nothing else.

## Adding a component

A new Zig component adds its own unit and integration roots to its build graph.
It then adds one row to unit, integration, coverage, and memory verification
where those lanes apply. The component does not add imports to another
component's root.

The reachability gate lists compiler-registered tests across every root. Its
aggregate counts must not fall below the baseline recorded by the active
workstream.
