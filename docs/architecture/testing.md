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

### kcov reads two of the eight components on Linux

kcov 43 collects the product line tables of only `runner` and `lib` on Linux,
reliably. The rest yield a Cobertura report with no classes in it at all —
usually. `deadline` and `s3` have each been seen contributing a couple of files
on one run and nothing on the next, from the same sources, so the set is not
merely small but unstable at its edge. Only `runner` and `lib` have collected on
every run observed, which is why they alone are required.

This is a kcov defect and not a filter or path mistake, on three pieces of
evidence: a kcov run with no `--include-pattern` or `--exclude-pattern` returns
nothing but `/opt/zig/lib/compiler_rt/*` for the affected binaries; their debug
information carries product units rooted inside the include path, which
`readelf` shows as `DW_AT_comp_dir` values under `src/`; and the same sources
measure all seven macOS components.

So the gate grades the union of the components that did collect and states
`measured over N of M components`, naming every component that captured
nothing, on success and on failure alike. `ZIG_COVERAGE_REQUIRED_COMPONENTS`
(`make/test.mk`, one definition site per platform) names those that must
collect; a required component contributing nothing fails the build, which is
the regression the earlier blanket refusal was written to catch.

**The Linux figure is not a whole-codebase figure, and it flatters.** What Linux
can read grades around 92% over 89 files, where all seven macOS components
measure about 90% over 565. Both the rate and the denominator move as edge
components flicker, so treat the Linux number as a floor check on `runner` and
`lib` rather than a codebase measurement, and take the codebase figure from a
macOS run. Read the published rate together with `zig_components_measured` and
`zig_measured_files`; a rise in the rate that comes with a fall in the file
count is a capture regression, not progress.
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
