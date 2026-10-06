# =============================================================================
# TEST-INTEGRATION-RUSTD — the Rust substrate against live Postgres + Dragonfly
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
#      exports `TEST_DATABASE_URL`, `TEST_DRAGONFLY_URL` and `TEST_DRAGONFLY_CA_CERT`,
#      and the suites read those names directly. This file used to resolve a URL
#      through a shell macro and hand it to cargo under a fourth, `AFD_`-prefixed
#      name; the rename bought nothing and cost a reader two files to answer
#      "where does this URL come from".
#   5. The recipe `cd`s into rustd/ rather than passing `--manifest-path`.
#      rustup selects a toolchain from the WORKING DIRECTORY, not from the
#      manifest, so `--manifest-path` builds the workspace with whatever
#      toolchain the machine defaults to — on the CI runner that is the image's
#      `stable`, not the version `rustd/rust-toolchain.toml` pins, and it moves
#      under us whenever the image is rebuilt. `make test-unit-rustd` has
#      always done it this way; this lane learned it the expensive way, on a
#      red CI run.

.PHONY: test-integration-rustd test-coverage-rustd test-coverage-rustd-merge rustd-coverage-shards _migrate-test-db

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
# afd_bench is a measuring instrument with separate manual bench-* targets.
# Its sampled datastore counters are not deterministic integration assertions.
# Keep it out of both datastore gates; retain the zero-tests guard for the
# actual service integration suite.

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
# The modules that must own the cluster while they run, as one filter both
# invocations share: the parallel lane skips them, the second runs only them,
# and neither can drift from the other.
#
# A test earns a place here by needing to observe the SERVER's whole client
# set. `integration_hub_exclusive` snapshots every node's `CLIENT LIST`, starts
# the hub, and kills the difference -- Dragonfly implements no `CLIENT KILL
# TYPE` and exposes no subscriber marker, so a diff is the only way to name the
# hub's connections at all. A sibling opening a connection inside that window
# would be killed by it, which is why this cannot ride the parallel lane.
#
# Still a hard gate: `_rust_lane` fails a selection that matched nothing, so a
# renamed or deleted module here fails the lane rather than silently passing.
EXCLUSIVE_FILTER := integration_hub_exclusive

test-integration-rustd: $(TEST_STATE_DEP) _migrate-test-db  ## Run the Rust substrate integration suite against compose Postgres + Dragonfly
	@command -v cargo >/dev/null 2>&1 || { echo "✗ cargo not found. Install via: mise install rust"; exit 1; }
	@echo "→ [rustd] Running the Rust integration suite against $(TEST_DATABASE_URL)..."; \
	$(call _rust_lane,rustd-integration.log,[rustd] integration suite,cargo test --workspace --exclude afd_bench --all-features --test "*" -- --ignored --skip $(EXCLUSIVE_FILTER))
	@echo "→ [rustd] Running the tests that need the cluster to themselves..."; \
	$(call _rust_lane,rustd-integration-exclusive.log,[rustd] integration suite (exclusive),cargo test --workspace --exclude afd_bench --all-features --test "*" -- --ignored --test-threads=1 $(EXCLUSIVE_FILTER))

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
# Raised 97 -> 98 on Indy's call (2026-09-02), during M181_004. That raise did
# NOT follow a measurement the way 96 -> 97 did: it was a target set ahead of
# the code, so the ratchet led rather than trailed.
#
# Returned 98 -> 97 on Indy's call, later the same day, and the distinction
# matters enough to write down. The rule this file states is that a floor never
# drops below a number a run ACHIEVED, because that is how a regression gets
# hidden. 98 was never achieved: the lane measured 97.0409% when the target was
# set and 97.2041% after the tests written against it. So this is not a floor
# retreating from its own history — it is an aspiration returning to the
# measurement, which is where every other number in this list came from.
#
# What that costs is honest: the gap to 98 was 295 lines at the last reading,
# and 97 does not close it. It concedes that reaching 98 is a milestone of its
# own — 295 lines spread across 240 files, whose cheap seams are spent and
# whose remainder is untaken-branch bodies and Err paths, one live-datastore
# fixture apiece. The 100% contract in the header remains authoritative and
# unchanged; this line is the ratchet, not the goal.
#
# Raised 97 -> 97.5 on Indy's call (2026-09-19), and it is the first number in
# this list that is not an integer. The lane reads it through
# `cargo llvm-cov report --fail-under-lines`, which parses a float and decides
# the verdict by exit code — the shell below only formats the sentence — so the
# half point is enforced rather than rounded. Verified both ways before the
# move: 97.5 exits 0 against this measurement and 99.9 exits 1.
#
# The measurement is 97.6994% — 41,362 of 42,336 lines, 974 missed — which is
# what makes this a ratchet onto ground already held rather than a second
# attempt at the 98 the paragraph above records failing twice. The margin is
# deliberate and small: 0.1994%, about 84 lines. A floor set at the
# measurement goes red on the next commit that touches an uncovered path, and a
# floor nobody can commit against is a floor someone switches off.
#
# What it concedes is the same thing 97 conceded, now with the remainder named.
# The gap to 98 is 128 lines, and the cheap seams really are spent: what is
# left is dominated by FAIL-OPEN arms — the branch a gate takes when the
# datastore will not answer, which by construction needs a datastore that will
# not answer — and by defensive arms over cases the types already exclude.
# `lease/coverage.rs` is the shape: all nine of its missed lines are
# `admit_unreadable`, reached only by breaking a read mid-question. Those are
# reachable, but not from the shared lane — a test that breaks the database
# breaks every suite running beside it. `Fixtures::create_isolated` is the seam
# that would do it honestly, on a private migrated database, and it is unused.
# 98 remains available behind that work; it is a milestone, not a knob.
RUSTD_COVERAGE_FLOOR ?= 97.5

# Test scaffolding leaves the DENOMINATOR, on Indy's call (2026-09-02).
#
# `crates/*/src/test_util.rs` is fixture code compiled only under the
# `test-util` feature -- `TestDatabase::shared` and the two like it. The lane
# measures with `--all-features`, so it was being graded as production surface:
# 143 lines, of which the suites happen to cover 127.
#
# That is the whole reason this is a MEASUREMENT fix and not a coverage win.
# Removing those lines removes 127 covered ones with them, so the number moves
# 97.1717% -> 97.2041% and the gap to the floor goes 308 lines -> 295. Nothing
# here is a shortcut to 98; it is only the denominator no longer counting
# fixtures as shipped code.
#
# Deleting the files instead was measured and rejected: 74 files and 117 call
# sites reach them, including every suite that depends on the shared-database
# contract this file's header describes.
#
# FILE granularity is all `--ignore-filename-regex` has, so this is exactly the
# three whole-file modules. The `#[cfg(feature = "test-util")]` helpers that sit
# INSIDE otherwise-production files -- afd_crypto's entropy seams, afd_db's
# migration and pool helpers, and a dozen more -- are still counted, because a
# filename filter cannot see a block. The exclusion is therefore partial by
# construction, and that is recorded here rather than discovered later by
# someone reconciling two numbers.
#
# The bench crate leaves the DENOMINATOR too, on Indy's call (2026-09-07):
# "the afd_bench/ crate must be ignored from codecov and the coverage we
# conduct, that is just an optional crate to measure bench and no where used
# in production". `crates/afd_bench` is a measuring instrument -- five lane
# binaries and the harness they share -- with no consumer in the workspace
# (`rustd/Cargo.toml` lists it as a member; nothing depends on it) and no path
# into a shipped binary. Its tests still RUN in this lane, so the lanes stay
# proven; their lines are no longer graded as production surface. On PR #667
# the crate's five `main`s and their shared preamble were 111 of the 167 unhit
# lines behind a 90.8% patch grade against the 97 floor. codecov.yml ignores
# the same path, so the two gates read one denominator.
#
# The toolbox's four kernel-lane files are proven by `make test-runner-kernel`
# (make/test-unit.mk): root on a real Linux kernel with loop devices and
# mounts, and it fails rather than skips. The runner behind THIS lane has
# neither, so the report it grades never hits them. On Pull Request (PR) #732
# at 669fc1d41 they were 154 of the 172 unhit changed lines (adopt.rs 70,
# loop_device.rs 51, admit.rs 24, kernel_mounter.rs 9) behind a 93.19% patch
# grade against the 99 floor; the rest of the diff graded 99.24%.
# `kernel_mounter.rs` was split out of holds.rs so a file-level ignore takes
# exactly it. Their tests still RUN in the kernel lane; their lines are no
# longer graded by a lane that cannot execute them. Excused on Indy's call
# (2026-10-06, PR #732 Session notes 2). codecov.yml ignores the same four
# paths, so the two gates read one denominator.
RUSTD_COVERAGE_IGNORE ?= (/src/test_util\.rs$$|/crates/afd_bench/|/crates/afr_sandbox/src/toolbox/(adopt|loop_device|admit|kernel_mounter)\.rs$$)

# The floor's verdict, carrying the number that decided it, decided ONCE.
#
# The lane runs as shards (below), and a shard's report covers only the tests it
# ran, so the floor cannot be read off any one of them. `scripts/rustd_coverage.py`
# merges the shards' lcov files line by line and grades the union. The same
# script grades a local unsharded run, so Continuous Integration and a developer
# read one judge.
#
# It replaces `cargo llvm-cov report --summary-only --fail-under-lines`, which can
# only grade a profile it holds; the merge job holds lcov files, not profiles.
# The two read the same numbers: LCOV's `LF:`/`LH:` records are llvm-cov's line
# denominator and numerator (verified equal on a probe crate, 2/5 = 40.00% both
# ways), and the script compares in exact decimals, so 97.5 is enforced rather
# than rounded. A red run prints the missed lines by crate, the answer to
# "where" that used to cost a second instrumented run.
#
# The judge grades the PATCH as well: the Rust lines this branch adds, against
# `RUSTD_PATCH_FLOOR`, which is codecov.yml's `rust-afd` patch target and moves
# with it. Codecov grades the same merged report; grading it here too means the
# lane's verdict names the unhit lines itself instead of waiting on a status
# nobody requires. The base is where this branch left `origin/main`;
# Continuous Integration passes the pull request's base explicitly. An empty
# base skips the patch grade (a checkout with no `origin/main` to measure from).
RUSTD_PATCH_FLOOR ?= 99
RUSTD_PATCH_BASE ?= $(shell git merge-base HEAD origin/main 2>/dev/null)
_RUSTD_COVERAGE_JUDGE = PYTHONDONTWRITEBYTECODE=1 python3 scripts/rustd_coverage.py --floor $(RUSTD_COVERAGE_FLOOR) \
  $(if $(RUSTD_PATCH_BASE),--patch-floor $(RUSTD_PATCH_FLOOR) --patch-base $(RUSTD_PATCH_BASE))

# Shards: three, each on its own runner with its own Postgres and Dragonfly,
# measured in parallel and graded together by `test-coverage-rustd-merge`.
#
#   runner     the Rust runner's crates, plus the daemon suite's
#              `integration_rust_runner::` modules: the runner against a live
#              daemon. A runner leases any fleet whose required tags it carries,
#              so on the shared lane it was offered every fleet the other
#              scenarios had left behind; on its own datastores it is offered its own.
#   daemon     `agentsfleetd`, every target, minus the runner's modules.
#   substrate  every other crate: the workspace minus the two above.
#
# The partition is total by construction. `substrate` is defined by exclusion,
# so a crate added later lands there without an edit, and `daemon` skips exactly
# the filter `runner` selects. No test can fall between shards, and a filter
# that matches nothing fails its invocation through `_rust_lane`'s zero-tests
# guard rather than passing empty.
#
# `RUSTD_SHARD` names one shard, which writes `lcov-<shard>.info` and its `.rev`
# sidecar and grades nothing. Unset, it is every shard, run in sequence into one
# profile and graded: the local full measurement, through the invocations
# Continuous Integration makes. Any other subset is refused, because grading a
# partial union is the failure this layout exists to rule out.
RUSTD_RUNNER_PACKAGES := afr_agent afr_executor afr_sandbox afr_supervisor agentsfleet_runner
RUSTD_DAEMON_PACKAGES := agentsfleetd
RUSTD_RUNNER_IN_DAEMON := integration_rust_runner::
RUSTD_SHARDS := runner daemon substrate
RUSTD_SHARD ?= $(RUSTD_SHARDS)

_RUSTD_COVER := cargo llvm-cov --no-report --all-features
_rustd_packages = $(foreach package,$(1),-p $(package))
_RUSTD_SUBSTRATE := --workspace $(foreach package,afd_bench $(RUSTD_RUNNER_PACKAGES) $(RUSTD_DAEMON_PACKAGES),--exclude $(package))
_RUSTD_SHARD_UNKNOWN := $(filter-out $(RUSTD_SHARDS),$(RUSTD_SHARD))
_RUSTD_SHARD_ALL := $(if $(filter-out $(RUSTD_SHARD),$(RUSTD_SHARDS)),,yes)
_RUSTD_SHARD_ONE := $(if $(filter 1,$(words $(RUSTD_SHARD))),yes,)
_RUSTD_SHARD_LCOV := $(RUSTD_DIR)/lcov-$(RUSTD_SHARD).info

# `cargo llvm-cov` reports only what actually ran. The integration tests are
# `#[ignore]`d, so a unit-only measurement sees every pool, stream and migrator
# line as uncovered — the code is exercised, just not by the run holding the
# instrument. Measuring here, with `--include-ignored`, puts the instrument
# where the datastores are. That is the milestone's stated route: reach the
# number, do not move the bar.
#
# Every test still runs ONCE: each lands in exactly one shard. Instrumenting the
# run the lane was already making is what keeps a full verification from
# executing every live-service test twice on two runners — the mistake the
# retired Zig graph made and then fixed. The lane migrates after the reset
# through `cargo llvm-cov run --no-report`, so the migrator's lines are measured
# and the daemon is built once, instrumented.
#
# `--no-report` on every pass is what carries the migrator's profile into the
# test runs: it is cargo-llvm-cov's accumulate mode, which skips the implicit
# clean and leaves the profraw for a later `report` to merge. The explicit
# `cargo llvm-cov clean --workspace` above is therefore the only clean, and it
# runs once, before any pass. `--no-clean` is NOT the way to spell this —
# cargo-llvm-cov refuses the pair outright ("error: --no-report may not be used
# together with --no-clean"), because --no-report already implies it. Verified
# on a probe crate: a `run --no-report` covering one function then a
# `--no-report` test pass covering another reported both (40% -> 60%), so
# nothing is lost by dropping it.
#
# The exclusive hub suite gets the cluster to itself here too. The single
# `--include-ignored` invocation this replaces ran it beside every other suite,
# the one thing `EXCLUSIVE_FILTER` says it must never do.
test-coverage-rustd: $(TEST_STATE_DEP)  ## Run both Rust test tiers under coverage against live datastores (RUSTD_SHARD=<one> for a Continuous Integration shard)
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { echo "✗ cargo-llvm-cov not found. Install via: cargo install cargo-llvm-cov"; exit 1; }
	@$(if $(_RUSTD_SHARD_UNKNOWN),echo "✗ [rustd] unknown RUSTD_SHARD: $(_RUSTD_SHARD_UNKNOWN) (known: $(RUSTD_SHARDS))"; exit 1,true)
	@$(if $(or $(_RUSTD_SHARD_ALL),$(_RUSTD_SHARD_ONE)),true,echo "✗ [rustd] RUSTD_SHARD is one shard or unset; '$(RUSTD_SHARD)' would grade part of the lane"; exit 1)
	@echo "→ [rustd] Removing stale instrumented workspace artifacts..."; \
	cd $(RUSTD_DIR) && cargo llvm-cov clean --workspace
	@echo "→ [infra] Applying migrations through the instrumented daemon..."; \
	cd $(RUSTD_DIR) && DATABASE_URL_MIGRATOR="$(TEST_DATABASE_URL)" \
	  cargo llvm-cov run --all-features --no-report --bin agentsfleetd -- migrate \
	  || { echo "✗ [infra] instrumented migrate failed"; exit 1; }
	@echo "✓ [infra] Instrumented schema applied"
ifneq ($(filter runner,$(RUSTD_SHARD)),)
	@echo "→ [rustd] Shard runner: the runner crates, then the runner against the daemon..."; \
	$(call _rust_lane,rustd-coverage-runner.log,[rustd] coverage: runner crates,$(_RUSTD_COVER) $(call _rustd_packages,$(RUSTD_RUNNER_PACKAGES)) -- --include-ignored); \
	$(call _rust_lane,rustd-coverage-runner-daemon.log,[rustd] coverage: runner against the daemon,$(_RUSTD_COVER) -p agentsfleetd --test daemon_suite -- --include-ignored $(RUSTD_RUNNER_IN_DAEMON))
endif
ifneq ($(filter daemon,$(RUSTD_SHARD)),)
	@echo "→ [rustd] Shard daemon: agentsfleetd without the runner's modules..."; \
	$(call _rust_lane,rustd-coverage-daemon.log,[rustd] coverage: daemon,$(_RUSTD_COVER) $(call _rustd_packages,$(RUSTD_DAEMON_PACKAGES)) -- --include-ignored --skip $(RUSTD_RUNNER_IN_DAEMON))
endif
ifneq ($(filter substrate,$(RUSTD_SHARD)),)
	@echo "→ [rustd] Shard substrate: every other crate, then the suites that need the cluster alone..."; \
	$(call _rust_lane,rustd-coverage-substrate.log,[rustd] coverage: substrate,$(_RUSTD_COVER) $(_RUSTD_SUBSTRATE) -- --include-ignored --skip $(EXCLUSIVE_FILTER)); \
	$(call _rust_lane,rustd-coverage-substrate-exclusive.log,[rustd] coverage: substrate (exclusive),$(_RUSTD_COVER) $(_RUSTD_SUBSTRATE) -- --include-ignored --test-threads=1 $(EXCLUSIVE_FILTER))
endif
ifeq ($(_RUSTD_SHARD_ALL),yes)
	@echo "→ [rustd] Rendering the run's profile, then grading the floor..."; \
	cd $(RUSTD_DIR) && cargo llvm-cov report --workspace \
	  --ignore-filename-regex '$(RUSTD_COVERAGE_IGNORE)' --lcov --output-path lcov-all.info \
	  || { echo "✗ [rustd] lcov report failed"; exit 1; }
	@$(_RUSTD_COVERAGE_JUDGE) --out $(RUSTD_DIR)/lcov.info $(RUSTD_DIR)/lcov-all.info
else
	@echo "→ [rustd] Rendering shard $(RUSTD_SHARD)'s profile; test-coverage-rustd-merge grades the floor..."; \
	cd $(RUSTD_DIR) && cargo llvm-cov report --workspace \
	  --ignore-filename-regex '$(RUSTD_COVERAGE_IGNORE)' --lcov --output-path lcov-$(RUSTD_SHARD).info \
	  || { echo "✗ [rustd] lcov report failed"; exit 1; }
	@git rev-parse HEAD > $(RUSTD_DIR)/lcov-$(RUSTD_SHARD).rev
	@echo "✓ [rustd] shard $(RUSTD_SHARD) measured: $(_RUSTD_SHARD_LCOV)"
endif

# The floor, graded once over every shard's report. Continuous Integration's
# merge job is the caller; a shard missing, or measured at another commit than
# this checkout, is refused rather than graded as a smaller lane.
test-coverage-rustd-merge:  ## Grade the Rust line floor once over every shard's lcov report
	@$(_RUSTD_COVERAGE_JUDGE) --revision "$$(git rev-parse HEAD)" --out $(RUSTD_DIR)/lcov.info \
	  $(foreach shard,$(RUSTD_SHARDS),$(RUSTD_DIR)/lcov-$(shard).info)

# The shard names, for the workflow's matrix: the list lives here once.
rustd-coverage-shards:  ## Print the Rust coverage lane's shard names
	@echo '$(RUSTD_SHARDS)'
