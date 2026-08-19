# =============================================================================
# TEST — aggregate orchestrator
# =============================================================================

include make/test-unit.mk
include make/test-infra.mk
include make/test-integration.mk
include make/acceptance.mk
include make/dry.mk
include make/bench.mk

# The GLOBAL cache is shared by every worktree on this machine; the LOCAL cache
# stays per-worktree. That is the split Zig intends: the global cache holds
# content-addressed build artifacts for the dependency graph (pg.zig, http.zig,
# nullclaw, ...), which are identical across worktrees of the same repo, while
# the local cache holds this checkout's own compilation.
#
# Pointing the global cache at $(CURDIR) gave each worktree a private copy and
# defeated it — four checkouts here held four 65-123 MB caches, each recompiling
# the same dependencies from scratch. Concurrent access is the case the global
# cache is built for (its default, ~/.cache/zig, is shared by every Zig project
# on the machine): entries are content-addressed and lock-protected.
#
# Both are `?=`, and an environment variable beats `?=`, so CI keeps overriding
# these to workspace-local paths it can cache between runs — see
# .github/workflows/test-integration.yml.
ZIG_GLOBAL_CACHE_DIR ?= $(HOME)/.cache/agentsfleet/zig-global-cache
ZIG_LOCAL_CACHE_DIR  ?= $(CURDIR)/.tmp/zig-local-cache
ZIG_COVERAGE_DIR ?= $(CURDIR)/coverage/zig
# Per-component kcov logs and exit statuses live under ZIG_COVERAGE_DIR beside
# the reports they explain. They were at a hardcoded relative `.tmp/`, which the
# lane self-tests share with a real run even though they redirect the coverage
# directory: a stubbed run truncated a real run's logs, and a real run's 57 KB
# log outlived a stubbed one, so each read the other's output and blamed the
# gate. The summary file keeps its own variable because CI reads that exact path
# (`.github/workflows/test.yml`) — the default must not move; only a test
# redirects it.
ZIG_COVERAGE_SUMMARY_FILE ?= .tmp/zig-coverage.txt
# ---------------------------------------------------------------------------
# Coverage floors, targets and denominator minimums — ONE definition site each.
# The checker accepts all of them only as arguments, so no recipe and no Python
# module can hold a second copy.
#
# Floors are ENFORCED and raise-only: move one in the same commit as the tests
# that measurably clear it, never ahead. 91 was once set ahead of the tests and
# gated nothing but red. Targets are PUBLISHED, never enforced — the gap between
# floor and target is printed every run so the destination stays visible without
# an unmet target turning the build red.
#
# Production lines only: the coverage target excludes test bodies AND test
# support from the denominator, so these are shares of shipped code. They read
# the unit lanes and the live-service integration suite merged, because those
# cover largely disjoint code and either alone understates the truth by tens of
# points.
# ---------------------------------------------------------------------------
# Was ZIG_COVERAGE_MIN_LINES, which named a percentage. It now sits beside a
# real measured-line minimum, and two variables a letter apart meaning entirely
# different things is how the wrong one gets edited.
#
# These figures are LOWER than the ones this gate published before, and the
# coverage did not regress. Test bodies written inside product files — 5,309
# lines, 17% of the old denominator — were being counted as shipped code, and a
# test body is ~100% covered by construction, so they lifted every rate by 1.7
# to 2.6 points. Removing them is the same rule that already drops `*_test.zig`
# files; it just reaches the blocks that live inside product sources.
ZIG_COVERAGE_MIN_PCT ?= 89
ZIG_COVERAGE_TARGET_PCT ?= 95
# Per-folder enforced floors. Measured on the union at the time each was set;
# they ratchet toward the targets below as tests land.
#
# Raised by the run that took the runner's unwind and give-up arms: measured
# 90.07 merged, 89.23 agentsfleetd, 95.18 runner, 95.02 lib over 9 of 9
# components. `runner` and `lib` have both reached the 95% quality bar, so each
# floor becomes that target — the folders are held there, not merely above where
# they happened to land. `agentsfleetd` held at 89 there because 89.23 did not
# clear 90.
#
# Raised again by the Dimension 4.3 tail (models/grants/billing/fleet-library/
# auth cursor and SSRF suites, plus the repair-verification double-free fix):
# measured 90.91 merged, 90.21 agentsfleetd (19898/22058) over 9 of 9
# components. `agentsfleetd` now clears its own 90 target from the same run, so
# floor becomes target here too, same as `runner`/`lib` above. Every floor here
# sits at or below its measured value, which is the only condition under which
# one may move.
#
# `runner` lowered back to 87 once this branch finally ran on real Linux CI
# instead of a dev Mac: measured 87.48% (2767/3163) there, not the 95.18% every
# prior measurement above was taken on macOS. `engine/{seccomp,landlock,
# cgroup}.zig` are Linux sandboxing enforcement — comptime-eliminated to stubs
# on macOS (contributing nothing to that denominator), but real code on Linux
# that only `sec_enforcement_integration_test.zig`'s privileged lane exercises,
# which this non-privileged coverage lane does not run. The 95 target stays;
# floor drops to what this lane can actually clear until that gap is closed by
# either a privileged coverage lane or tests that hold without one.
#
# `lib` corrected the same way in the same run: measured 94.94% on Linux
# (826/870), not the 95.02% on record above, taken on macOS like everything
# else in this file until this branch's first real CI cycle. `ZIG_COVERAGE_
# MIN_PCT` above (90 → 89) moved for the identical reason — it is a weighted
# average that folded in `runner`'s and `lib`'s inflated macOS numbers too.
# Both reproduced byte-identical across two separate CI runs, so this is
# measurement, not flake. Floors ratchet back up once each folder's
# Linux-measured rate clears the new mark, same as every earlier line here.
ZIG_COVERAGE_FOLDER_FLOORS ?= agentsfleetd=90 runner=87 lib=94
# The quality bar for every product folder. 95 everywhere except the daemon,
# which Indy cut to 91 on Aug 16, 2026, then to 90 the same day once the
# session's remaining commits closed most of the 91 gap on their own — he'd
# rather bank the PR at 90 than fund another round of big-file splits for the
# last point. 90 is a waypoint, not a lowered bar: the merged 95 above still
# implies the daemon eventually goes past it, because the daemon is 86% of the
# denominator.
ZIG_COVERAGE_FOLDER_TARGETS ?= agentsfleetd=90 runner=95 lib=95
# One floor under the shape of the whole report, deliberately NOT one per
# component. The failure being caught is collapse — kcov once returned 24 files
# where the tree holds 558 — and a pair of numbers at roughly half the measured
# figures catches that by a mile. Per-component minimums were tried and cut:
# they were fourteen numbers to maintain, they duplicated the
# require-component assertion that already fails a component contributing
# nothing, and they turned every honest deletion of dead code into a red gate.
# Set these low on purpose. They are a collapse alarm, not a growth ratchet.
ZIG_COVERAGE_MIN_FILES ?= 300
ZIG_COVERAGE_MIN_MEASURED_LINES ?= 18000
# Product roots that must carry a measured line, whatever the rate. A union at
# 98% holding one tree is not a measurement of the codebase.
ZIG_COVERAGE_REQUIRED_ROOTS ?= agentsfleetd runner lib
# Components whose reports MUST carry measured lines, one definition site per
# platform. A component that collects today and stops fails the gate, instead of
# quietly shrinking the denominator.
#
# Linux carried a short list while Zig's self-hosted backend emitted debug info
# libdw refuses. Test binaries now compile through LLVM, which fixes it at the
# source (docs/architecture/testing.md §Coverage). The list ratchets on evidence:
# add a component in the commit where a green run shows it collecting.
#
# Ratcheted to the full Linux set on the run that earned it — job 94963891177
# reported `measured over 8 of 8 components — every component collected`, every
# one carrying lines (agentsfleetd 26392, integration 23104, runner 4588,
# runner_integration 4136, deadline 307, lib 594, logging 276, s3 28). Naming
# all eight is what keeps the LLVM fix honest: with only `runner lib` required,
# the six that were silently dark could go dark again and the gate would stay
# green. runner_integration is Linux-only, which is why this list is longer.
ifeq ($(shell uname -s),Linux)
ZIG_COVERAGE_REQUIRED_COMPONENTS ?= agentsfleetd runner lib logging deadline s3 runner_integration integration
else
# `lifecycle` and `runner_integration` are required here and not above for the
# reason this list states: evidence, in the commit it arrives. A macOS run
# showed lifecycle collecting 21,686 lines over 446 files, and the runner
# integration suite — long marked Linux-only, though its worker-pool fork tests
# run fine on macOS — 305 passed with the real fork→execute→report path
# collected. Each joins the Linux list on the CI run that shows the same. The
# run-marker assertion in the recipe is the other half — it proves the
# lifecycle test executed, where this proves the report carried lines.
ZIG_COVERAGE_REQUIRED_COMPONENTS ?= agentsfleetd runner lib logging deadline s3 integration lifecycle runner_integration
endif
# The boot -> SIGTERM -> drain proof is the only test that drives the real
# `serve.run`, and it is far too invasive to interleave with the ~2000 tests in
# the shared integration binary: it perturbs process-global state and
# destabilises unrelated tests. Two lanes therefore run it alone, filtered, with
# the isolation variable set — the leak gate and the coverage gate — so both
# need the same three strings. One definition site each: a filter that silently
# stops matching runs nothing, and both lanes assert the marker for exactly that
# reason.
LIFECYCLE_ISOLATION_ENV ?= AGENTSFLEET_LIFECYCLE_ISOLATED
LIFECYCLE_TEST_FILTER ?= daemon boot -> SIGTERM -> drain
LIFECYCLE_RUN_MARKER ?= SERVE_LIFECYCLE_BOOT_DRAIN_RAN
# ---------------------------------------------------------------------------
# Component inventory — split by which lane executes it, one definition site.
#
# The split IS the ownership. `test-coverage-zig` runs the unit components;
# `test-integration` runs the live ones, because it is the lane that already
# stands up datastores, migrates them and builds the runner binary. Before the
# split both lanes ran the daemon integration suite — the coverage lane under
# kcov and the integration lane bare — so a full verification paid for it twice
# and Continuous Integration (CI) paid for it on two runners.
#
# `runner_integration` is a unit component despite its name: it drives the
# runner's own build graph and needs no datastore.
ZIG_COVERAGE_UNIT_COMPONENTS ?= agentsfleetd:agentsfleetd-tests runner:agentsfleet-runner-tests lib:agentsfleet-lib-tests logging:agentsfleet-logging-tests deadline:agentsfleet-call-deadline-tests s3:agentsfleet-s3-tests runner_integration:agentsfleet-runner-integration-tests
# The two live components share one binary: `integration` runs it unfiltered and
# `lifecycle` runs it rebuilt under the boot-to-drain filter, which the
# unfiltered run skips. Two executions, no test run twice.
ZIG_COVERAGE_LIVE_COMPONENTS ?= integration lifecycle
ZIG_INTEGRATION_TEST_BIN ?= agentsfleetd-integration-tests
ZIG_COVERAGE_UNIT_NAMES = $(foreach pair,$(ZIG_COVERAGE_UNIT_COMPONENTS),$(firstword $(subst :, ,$(pair))))
ZIG_COVERAGE_ALL_NAMES = $(ZIG_COVERAGE_UNIT_NAMES) $(ZIG_COVERAGE_LIVE_COMPONENTS)

# What a failing component's log is grepped for when a lane reports it. The Zig
# test runner puts its verdict on its OWN line — `FAIL (TestExpectedEqual)` —
# while the test's name and the assertion message sit on the line ABOVE, so the
# match is taken with `grep -B 1` or the report names nothing. Every other
# alternative is anchored: an unanchored `panic` matched the *passing* test
# "…instead of @intCast-panicking" and printed it as the failure for a whole
# round of red CI. Both lanes grep the same way, so both read from here.
ZIG_TEST_FAILURE_GREP = (^|\.\.\.)FAIL\b|^error: .* failed:|error return trace|^thread [0-9]+ panic|^panic:
# Dropped before the `-B 1` window is taken: valgrind writes its own commentary
# (`--PID-- …`, `==PID== …`) into the same stream, and a single interleaved
# warning is enough to push the failing test's name out of the window.
ZIG_TEST_LOG_NOISE = ^--[0-9]+--|^==[0-9]+==

# The environment every kcov component runs under, whichever lane runs it. One
# definition site so a component measures the same thing after it moves lanes.
# `$$db_url` and `$$redis_url` are resolved by the recipe shell at run time.
ZIG_COVERAGE_ENV = \
	 LIVE_DB=1 \
	 TEST_DATABASE_URL="$$db_url" \
	 TEST_REDIS_TLS_URL="$$redis_url" \
	 REDIS_URL_API="$$redis_url" \
	 REDIS_TLS_CA_CERT_FILE="$(TEST_REDIS_TLS_CA_CERT)" \
	 AGENTSFLEET_RUNNER_BIN="$(CURDIR)/zig-out/bin/agentsfleet-runner" \
	 AGENTSFLEET_QSTASH_LIVE_URL="$(QSTASH_DEV_URL_LOCAL)" \
	 AGENTSFLEET_QSTASH_LIVE_TOKEN="$(QSTASH_DEV_TOKEN_LOCAL)"
# `--exclude-pattern` keeps test bodies OUT of the denominator. They are ~23k of
# the measured lines and are themselves ~90% covered — counting them inflated the
# figure by roughly seven points and, worse, made the gate satisfiable by writing
# more test files rather than covering more product.
ZIG_COVERAGE_KCOV = kcov --clean --include-pattern="$(CURDIR)/src" --exclude-pattern=_test.zig

# ---------------------------------------------------------------------------
# Producer evidence. Each coverage lane records what it measured and what it
# measured it against; the grade refuses anything that no longer fits. A
# `coverage/zig/` tree outlives a branch switch, a toolchain bump and a rebuild,
# so "the reports are on disk" is not evidence that they describe this build.
# ---------------------------------------------------------------------------
ZIG_EVIDENCE_DIR ?= $(CURDIR)/.tmp/verification
ZIG_EVIDENCE_UNIT ?= $(ZIG_EVIDENCE_DIR)/unit.json
ZIG_EVIDENCE_INTEGRATION ?= $(ZIG_EVIDENCE_DIR)/integration.json
# The sources that reach the measured binaries. A digest over these is what
# makes evidence from another commit refuse itself.
ZIG_EVIDENCE_SOURCE_ARGS = \
	 --source-path src --source-path schema \
	 --source-path build.zig --source-path build.zig.zon --source-path build_runner.zig
# The graph the evidence was recorded against: change any of it and the union is
# grading a different question, so recorded evidence must not survive the change.
ZIG_EVIDENCE_GRAPH_ARGS = \
	 --graph "$(ZIG_COVERAGE_UNIT_COMPONENTS)" \
	 --graph "$(ZIG_COVERAGE_LIVE_COMPONENTS)" \
	 --graph "$(ZIG_COVERAGE_REQUIRED_COMPONENTS)" \
	 --graph "$(ZIG_COVERAGE_REQUIRED_ROOTS)" \
	 --graph "$(ZIG_COVERAGE_FOLDER_FLOORS)" \
	 --graph "$(ZIG_COVERAGE_MIN_PCT)"
ZIG_EVIDENCE_ARGS = --repo-root "$(CURDIR)" $(ZIG_EVIDENCE_SOURCE_ARGS) $(ZIG_EVIDENCE_GRAPH_ARGS)
ZIG_EVIDENCE_RECORD = python3 scripts/verification_evidence.py record $(ZIG_EVIDENCE_ARGS)

# Use baseline CPU so valgrind can execute SHA/AVX instructions it can't emulate.
MEMLEAK_CPU ?= baseline

.PHONY: test-unit-all test-coverage-grade

test-unit-all: test-unit-agentsfleetd test-unit-agentsfleet-runner test-unit-agentsfleet-lib test-coverage-all  ## Run all unit lanes (Zig + multi-package coverage)
	@echo "✓ All unit lanes passed"

# The merged floor has one owner, and it is neither producer. Neither lane can
# see the union on its own — the unit lane no longer runs the live components and
# the integration lane never runs the unit ones — so grading from inside either
# would be grading half a codebase and calling it the whole one.
#
# It validates before it grades. Every component report on disk is accepted only
# when its producer's manifest still matches this build's sources, toolchain,
# component inventory and platform, and only when every component in the
# inventory was produced exactly once. Omission would shrink the denominator
# silently; duplication would mean two lanes ran one binary, which is the
# duplication this split removed.
test-coverage-grade:  ## Validate both producers' evidence, then grade the merged Zig coverage union
	@python3 scripts/verification_evidence.py validate \
	  $(ZIG_EVIDENCE_ARGS) \
	  --manifest "test-coverage-zig:$(ZIG_EVIDENCE_UNIT)" \
	  --manifest "test-integration:$(ZIG_EVIDENCE_INTEGRATION)" \
	  $(foreach name,$(ZIG_COVERAGE_ALL_NAMES),--expect-component $(name))
	@set -eu; \
	 component_flags=""; \
	 for name in $(ZIG_COVERAGE_ALL_NAMES); do \
	   component_flags="$$component_flags --component $$name"; done; \
	 for name in $(ZIG_COVERAGE_REQUIRED_COMPONENTS); do \
	   component_flags="$$component_flags --require-component $$name"; done; \
	 for name in $(ZIG_COVERAGE_REQUIRED_ROOTS); do \
	   component_flags="$$component_flags --require-root $$name"; done; \
	 for pair in $(ZIG_COVERAGE_FOLDER_FLOORS); do \
	   component_flags="$$component_flags --folder-floor $$pair"; done; \
	 for pair in $(ZIG_COVERAGE_FOLDER_TARGETS); do \
	   component_flags="$$component_flags --folder-target $$pair"; done; \
	 python3 scripts/check_zig_coverage.py \
	   --coverage-dir "$(ZIG_COVERAGE_DIR)" \
	   $$component_flags \
	   --min-pct "$(ZIG_COVERAGE_MIN_PCT)" \
	   --target-pct "$(ZIG_COVERAGE_TARGET_PCT)" \
	   --min-files "$(ZIG_COVERAGE_MIN_FILES)" \
	   --min-lines "$(ZIG_COVERAGE_MIN_MEASURED_LINES)" \
	   --merged-report "$(ZIG_COVERAGE_DIR)/merged" \
	   --repo-root "$(CURDIR)" \
	   --summary-file "$(ZIG_COVERAGE_SUMMARY_FILE)" \
	 || { \
	   echo "--- kcov stderr tails (why a capture came back empty) ---"; \
	   for f in $(ZIG_COVERAGE_DIR)/kcov-*.log; do \
	     echo "── $$f"; tail -n 12 "$$f"; \
	   done; \
	   exit 1; \
	 }
