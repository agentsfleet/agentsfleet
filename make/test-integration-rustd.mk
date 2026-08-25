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
#   1. The exit code travels through a FILE, not `set -o pipefail`. This recipe
#      runs under /bin/sh, which is dash on the CI runner, and dash has no
#      pipefail. Piping into `tee` without it reports tee's status, which turns
#      every failing suite into a green lane.
#   1a. `test-coverage-rustd` captures that verdict into `$verdict` and exits on
#      it, because a LAST LINE decides a recipe. It used to end
#      `…rustd_lane_result.py …; echo "report at …"`, so the recipe's status was
#      the echo's — always 0. The script printed `✗ Coverage run failed`, make
#      called it a success, and CI only went red further down when Codecov could
#      not find the `lcov.info` a failed run never wrote. That is item 1 again,
#      one line lower: the lane knew, and could not say so. It matters more here
#      than anywhere else in this file, because `make test-coverage-rustd` is
#      what `.oracle/orly.json` declares for `verify.integration` — a gate whose
#      green was unfalsifiable.
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

.PHONY: test-integration-rustd test-coverage-rustd

# Integration tests are marked `#[ignore]` in the source and run ONLY here, via
# `--ignored`. That is the cargo-native gate and it costs nothing at unit time:
# `make test-unit-rustd` still COMPILES every one of them (so they are type-
# checked and linted like the rest), lists them as ignored, and runs none —
# which is what keeps live Postgres off the fast lane. Each ignore reason names
# this target, so a developer who runs one directly is told where it belongs.

test-integration-rustd: $(TEST_STATE_DEP)  ## Run the Rust substrate integration suite against compose Postgres + Redis
	@command -v cargo >/dev/null 2>&1 || { echo "✗ cargo not found. Install via: mise install rust"; exit 1; }
	@echo "→ [rustd] Running the Rust integration suite against $(TEST_DATABASE_URL)..."; \
	mkdir -p "$(CURDIR)/.tmp"; \
	tally="$(CURDIR)/.tmp/rustd-integration.log"; \
	code="$(CURDIR)/.tmp/rustd-integration.status"; \
	rm -f "$$tally" "$$code"; \
	{ cd $(RUSTD_DIR) && cargo test --workspace --all-features \
	      -- --ignored 2>&1; \
	  echo $$? > "$$code"; } | tee "$$tally"; \
	python3 "$(CURDIR)/scripts/rustd_lane_result.py" \
	  --tally "$$tally" --status "$$(cat "$$code")" \
	  --label "[rustd] Integration suite"

# The ONE invocation that executes both tiers, and therefore the one that
# measures them.
#
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
test-coverage-rustd: $(TEST_STATE_DEP)  ## Run both Rust test tiers under coverage against live datastores
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { echo "✗ cargo-llvm-cov not found. Install via: cargo install cargo-llvm-cov"; exit 1; }
	@echo "→ [rustd] Measuring both test tiers against $(TEST_DATABASE_URL)..."; \
	mkdir -p "$(CURDIR)/.tmp"; \
	tally="$(CURDIR)/.tmp/rustd-coverage.log"; \
	code="$(CURDIR)/.tmp/rustd-coverage.status"; \
	rm -f "$$tally" "$$code"; \
	{ cd $(RUSTD_DIR) && cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info \
	      -- --include-ignored 2>&1; \
	  echo $$? > "$$code"; } | tee "$$tally"; \
	python3 "$(CURDIR)/scripts/rustd_lane_result.py" \
	  --tally "$$tally" --status "$$(cat "$$code")" \
	  --label "[rustd] Coverage run"; verdict=$$?; \
	echo "  report at $(RUSTD_DIR)/lcov.info"; \
	exit $$verdict
