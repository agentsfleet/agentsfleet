# =============================================================================
# TEST-INTEGRATION-RUSTD — the Rust substrate against live Postgres + Redis
# =============================================================================
# M175 §6 deleted `make/test-integration.mk` with the rest of the Zig gating.
# The datastores did not go away with it: `make/test-infra.mk` survived, because
# it is the disposable-environment half — what boots, where it listens, and how
# state is reset. This file is the lane that consumes it for the Rust port.
#
# Named `test-integration-rustd` rather than reclaiming the freed
# `test-integration`: that name meant "the Zig daemon suite" for two years, and
# a target that silently inherits a retired meaning is how a green run gets read
# as a claim it never made.
#
# Three things in the recipe are load-bearing and easy to "simplify" away:
#
#   1. The Python wrapper OWNS the child process. This recipe runs under
#      /bin/sh, which is dash on the CI runner, and dash has no `pipefail`.
#      Piping through `tee` would report tee's status; writing Cargo's status to
#      a side file instead made disk exhaustion replace the original failure.
#      The wrapper streams and tallies output while retaining the child's status
#      in memory, so neither shell feature nor writable diagnostic file decides
#      whether the lane passed.
#   2. The lane fails when the suite reports ZERO passing tests. A selection
#      that matches nothing exits 0, and "0 tests ran" is indistinguishable from
#      "everything passed" by exit status alone — the Zig lane learned this the
#      expensive way (it ran green for a week against a dead port).
#   3. `$(TEST_STATE_DEP)` — a gate run drops schemas and flushes Redis first,
#      while `KEEP_TEST_STATE=1` keeps the inner loop fast. Same contract the
#      Zig lane had; CI never sets the escape hatch.
#   4. The three service knobs are NOT passed on the command line. `test-infra.mk`
#      exports `TEST_DATABASE_URL`, `TEST_REDIS_URL` and `TEST_REDIS_CA_CERT`,
#      and the suites read those names directly. This file used to resolve a URL
#      through a shell macro and hand it to cargo under a fourth, `AFD_`-prefixed
#      name; the rename bought nothing and cost a reader two files to answer
#      "where does this URL come from".
#   5. The recipe `cd`s into rustd/ rather than passing `--manifest-path`.
#      rustup selects a toolchain from the WORKING DIRECTORY, not from the
#      manifest, so `--manifest-path` builds the workspace with whatever
#      toolchain the machine defaults to — on the CI runner that is the image's
#      `stable`, not the 1.98.0 this repository pins, and it moves under us
#      whenever the image is rebuilt. `make test-unit-rustd` has always done it
#      this way; this lane learned it the expensive way, on a red CI run.

.PHONY: test-integration-rustd test-coverage-rustd _migrate-test-db

# The schema, applied ONCE for the whole lane.
#
# `$(TEST_STATE_DEP)` drops the schemas and says "migrations will rebuild on
# next step". This is that step, and it is the step the port had been skipping:
# every test built a database of its own and applied all forty-seven
# `schema/*.sql` files into it, which at a hundred and forty-three tests is
# about six thousand seven hundred migration applications to produce one schema
# a hundred and forty-three times. That was the whole of the lane's runtime.
#
# The Zig harness never did this. Its contract was one line — "Runs against the
# LIVE test database. Never creates temp tables." — and a hundred and forty-five
# integration files honoured it. `afd_db::test_util::TestDatabase::shared` is
# that contract restored; see that module on what replaces the isolation.
#
# Through the daemon's own `migrate` subcommand rather than a bespoke recipe, so
# the lane applies the schema the way a deployment does — including the ledger,
# the advisory lock, and the refusal to run against a version this binary does
# not know. A second path to the same schema is a second thing to drift.
_migrate-test-db:
	@echo "→ [infra] Applying migrations once, for the whole lane..."; \
	cd $(RUSTD_DIR) && DATABASE_URL_MIGRATOR="$(TEST_DATABASE_URL)" \
	  cargo run --quiet --bin agentsfleetd -- migrate \
	  || { echo "✗ [infra] migrate failed"; exit 1; }
	@echo "✓ [infra] Schema applied"

# Integration tests are marked `#[ignore]` in the source and run ONLY here, via
# `--ignored`. That is the cargo-native gate and it costs nothing at unit time:
# `make test-unit-rustd` still COMPILES every one of them (so they are type-
# checked and linted like the rest), lists them as ignored, and runs none —
# which is what keeps live Postgres off the fast lane. Each ignore reason names
# this target, so a developer who runs one directly is told where it belongs.

# ONE guard, both lanes — $(call _rust_lane,<tally-name>,<label>,<command...>)
#
# Two ways a Rust lane reports success it did not earn, and this closes both:
#
#   1. The child failed and the pipe swallowed it. `tee` is the last command in
#      the pipeline, so `$$?` is tee's status, not cargo's. `bash -o pipefail`
#      is what makes the pipeline carry the child's failure instead — and it is
#      spelled `bash` explicitly because make runs recipes under `/bin/sh`,
#      which is dash on the Continuous Integration image and has no pipefail.
#   2. Nothing ran. A `--ignored` selection matching nothing exits 0 and prints
#      `0 passed`, which reads exactly like a pass. So the passing counts are
#      summed across every `test result:` line and a zero total is a failure,
#      whatever the exit status said.
#
# The tally file is a diagnostic convenience. Losing it may cost a developer an
# artifact; it can never cost the exit status, which is read from the pipeline
# rather than from anything written to disk.
define _rust_lane
mkdir -p "$(CURDIR)/.tmp"; \
tally="$(CURDIR)/.tmp/$(1)"; \
rm -f "$$tally"; \
bash -o pipefail -c 'cd "$(RUSTD_DIR)" && { $(WITH_PROGRESS) "$(2)" -- $(3) ; } 2>&1 | tee "$$0"' "$$tally"; \
status=$$?; \
ran=$$(sed -n 's/.* \([0-9][0-9]*\) passed.*/\1/p' "$$tally" 2>/dev/null | awk '{ t += $$1 } END { print t + 0 }'); \
if [ "$$status" -ne 0 ]; then \
  echo "✗ $(2) failed (exit $$status)"; \
  exit "$$status"; \
elif [ "$$ran" -eq 0 ]; then \
  echo "✗ $(2) ran no tests — a selection matching nothing is not a pass"; \
  exit 1; \
else \
  echo "✓ $(2) — $$ran passed"; \
fi
endef

# The wrapper merges the command's stderr into stdout itself. Its diagnostic log
# is best-effort: losing that file may lose a convenience artifact, never the
# child's exit status or the passing-test count.
test-integration-rustd: $(TEST_STATE_DEP) _migrate-test-db  ## Run the Rust substrate integration suite against compose Postgres + Redis
	@command -v cargo >/dev/null 2>&1 || { echo "✗ cargo not found. Install via: mise install rust"; exit 1; }
	@echo "→ [rustd] Running the Rust integration suite against $(TEST_DATABASE_URL)..."; \
	$(call _rust_lane,rustd-integration.log,[rustd] integration suite,cargo test --workspace --all-features --test "*" -- --ignored)

# The ONE invocation that executes both tiers, and therefore the one that
# measures them.
#
# The line floor this lane enforces, and the reason it is not 100.
#
# The repository's committed contract is 100% and remains the target; the spec
# carrying this work says so itself ("an implementation checkpoint while the
# committed 100% contract remains authoritative"). What this is is a RATCHET:
# a floor set to the coverage already achieved, so the lane can go green on
# work that did not regress while the remaining gap is closed by later
# milestones.
#
# 96 comes from a measured 96.0219% -- 25,224 of 26,269 lines, 1,045 missed
# across 153 files, the largest being afd_fleet (231), afd_gate (100) and
# afd_credential (95). That reading is from an earlier run and the floor was
# set from it deliberately rather than by re-measuring, on the user's call.
#
# A ratchet only moves UP. Lowering this number to make a red lane green is
# the thing it exists to prevent: raise it whenever a run beats it, and never
# reduce it without recording why, here.
# Raised 96 -> 97 on Indy's call (2026-08-31): the last Pull Request measured
# 97, so the ratchet moves up to meet it. Not re-measured here — the coverage
# lane needs the live datastores, and the same provenance rule the 96 was set
# under applies: the number is the user's reading, recorded rather than
# re-derived.
RUSTD_COVERAGE_FLOOR ?= 97

# The floor's verdict, carrying the number that decided it.
#
# This lane used to grade itself with `cargo llvm-cov --lcov --output-path
# lcov.info --fail-under-lines N`, and that combination reports a failure the
# reader cannot act on: cargo-llvm-cov 0.9.0 writes the lcov file, flips its
# internal error flag, and exits 1 WITHOUT printing a percentage — the lcov
# exporter has no summary to print one in. The Continuous Integration log for a
# red run therefore ended:
#
#     Finished report saved to lcov.info
#     ✗ [rustd] coverage run failed (exit 1)
#
# which names neither the measurement nor the floor it missed, and leaves
# "coverage fell" indistinguishable from "the exporter broke". Answering "by how
# much, and where" then costs a second full instrumented run.
#
# So the grading moves to a `--summary-only` report over the SAME profile: that
# form does print the per-file table and the TOTAL row, and it still carries the
# `--fail-under-lines` verdict in its exit status, so the tool remains the judge.
# The percentage in the ✗/✓ line is summed from `lcov.info` rather than from a
# third `report` invocation — LCOV's `LF:`/`LH:` records ARE llvm-cov's line
# denominator and numerator (verified equal on a probe crate: 2/5 = 40.00% by
# both routes), so the file already on disk answers it for free.
#
# The per-crate rollup fires only on a red run. A floor miss is spread across
# crates, and the first question after "by how much" is always "where" — that is
# the list, sorted by the lines each crate is missing.
define _rustd_coverage_verdict
mkdir -p "$(CURDIR)/.tmp"; \
summary="$(CURDIR)/.tmp/rustd-coverage-summary.txt"; \
lcov="$(CURDIR)/$(RUSTD_DIR)/lcov.info"; \
cd "$(RUSTD_DIR)" && cargo llvm-cov report --workspace \
  --summary-only --fail-under-lines $(RUSTD_COVERAGE_FLOOR) > "$$summary" 2>&1; \
verdict=$$?; \
cat "$$summary"; \
set -- $$(awk -F: '/^LF:/ { f += $$2 } /^LH:/ { h += $$2 } END { if (f == 0) print "0 0 0.0000"; else printf "%d %d %.4f\n", h, f, h * 100 / f }' "$$lcov" 2>/dev/null); \
covered=$${1:-0}; total=$${2:-0}; pct=$${3:-0.0000}; missed=$$((total - covered)); \
if [ "$$verdict" -ne 0 ]; then \
  echo "✗ [rustd] line coverage $$pct% is below the $(RUSTD_COVERAGE_FLOOR)% floor — $$covered of $$total lines covered, $$missed missed"; \
  echo "  the floor is a ratchet: write the tests. Lowering RUSTD_COVERAGE_FLOOR is the thing it exists to prevent."; \
  echo "  missed lines by crate:"; \
  awk -F: '/^SF:/ { file = $$2 } /^LF:/ { f = $$2 } /^LH:/ { m = f - $$2; if (m > 0) { crate = file; sub(/.*\/crates\//, "", crate); sub(/\/.*/, "", crate); if (crate == file) crate = "(workspace root)"; miss[crate] += m } } END { for (c in miss) printf "%d\t%s\n", miss[c], c }' "$$lcov" 2>/dev/null \
    | sort -rn | head -12 | awk -F'\t' '{ printf "    %6d  %s\n", $$1, $$2 }'; \
else \
  echo "✓ [rustd] line coverage $$pct% meets the $(RUSTD_COVERAGE_FLOOR)% floor — $$covered of $$total lines covered, $$missed missed"; \
fi; \
echo "  report at $$lcov"; \
exit $$verdict
endef

# `cargo llvm-cov` reports only what actually ran. The integration tests are
# `#[ignore]`d, so a unit-only measurement sees every pool, stream and migrator
# line as uncovered — the code is exercised, just not by the run holding the
# instrument. Measuring here, with `--include-ignored`, puts the instrument
# where the datastores are. That is the milestone's stated route: reach the
# number, do not move the bar.
#
# It runs the suite ONCE. Instrumenting the run the lane was already making is
# what keeps a full verification from executing every live-service test twice
# on two runners — the mistake the retired Zig graph made and then fixed.
# The coverage lane still migrates after the reset, but it does so through
# `cargo llvm-cov run --no-report`. The old `_migrate-test-db` prerequisite
# built the full daemon normally and the coverage invocation then built the
# same graph again with instrumentation.
#
# `--no-report` on BOTH passes is what carries the migrator's profile into the
# test run: it is cargo-llvm-cov's accumulate mode, which skips the implicit
# clean and leaves the profraw for a later `report` to merge. The explicit
# `cargo llvm-cov clean --workspace` above is therefore the only clean, and it
# runs once, before either pass. `--no-clean` is NOT the way to spell this —
# cargo-llvm-cov refuses the pair outright ("error: --no-report may not be used
# together with --no-clean"), because --no-report already implies it. Verified
# on a probe crate: a `run --no-report` covering one function then a
# `--no-report` test pass covering another reported both (40% -> 60%), so
# nothing is lost by dropping it.
test-coverage-rustd: $(TEST_STATE_DEP)  ## Run both Rust test tiers under coverage against live datastores
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { echo "✗ cargo-llvm-cov not found. Install via: cargo install cargo-llvm-cov"; exit 1; }
	@echo "→ [rustd] Removing stale instrumented workspace artifacts..."; \
	cd $(RUSTD_DIR) && cargo llvm-cov clean --workspace
	@echo "→ [infra] Applying migrations through the instrumented daemon..."; \
	cd $(RUSTD_DIR) && DATABASE_URL_MIGRATOR="$(TEST_DATABASE_URL)" \
	  cargo llvm-cov run --all-features --no-report --bin agentsfleetd -- migrate \
	  || { echo "✗ [infra] instrumented migrate failed"; exit 1; }
	@echo "✓ [infra] Instrumented schema applied"
	@echo "→ [rustd] Measuring both test tiers against $(TEST_DATABASE_URL)..."; \
	$(call _rust_lane,rustd-coverage.log,[rustd] coverage run,cargo llvm-cov --workspace --all-features --no-report -- --include-ignored)
	@echo "→ [rustd] Rendering lcov.info from the run's profile..."; \
	cd $(RUSTD_DIR) && cargo llvm-cov report --workspace --lcov --output-path lcov.info \
	  || { echo "✗ [rustd] lcov report failed"; exit 1; }
	@$(_rustd_coverage_verdict)
