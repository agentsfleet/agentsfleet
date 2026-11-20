# The Rust lanes against live Postgres + NATS (make/test-infra.mk). Recipes `cd`
# into rustd/ rather than pass `--manifest-path`: rustup picks the toolchain from
# the working directory, and the pinned one is in rustd/rust-toolchain.toml.
.PHONY: test-integration-rustd test-coverage-rustd test-coverage-rustd-merge rustd-coverage-shards _migrate-test-db

# The schema, once per lane, through the daemon's own `migrate`, so it applies
# the way a deployment does. The reset is its prerequisite, not a sibling, so
# `make -j` cannot run the two at once.
# The API login is idempotent (roles outlive the reset); its bounds equal
# afd_db::login's constants, which `the_api_login_runs_under_the_intake_bounds` reads back.
define _api_login
docker compose exec -T postgres psql -U agentsfleet -d agentsfleetdb -v ON_ERROR_STOP=1 -q -c \
  "DO \$$\$$ BEGIN CREATE ROLE $(TEST_DATABASE_API_LOGIN) LOGIN PASSWORD '$(TEST_DATABASE_API_LOGIN)' IN ROLE api_runtime; EXCEPTION WHEN duplicate_object OR unique_violation THEN NULL; END \$$\$$" \
  >/dev/null || { echo "✗ [infra] API login failed"; exit 1; }; \
docker compose exec -T postgres psql -U agentsfleet -d agentsfleetdb -v ON_ERROR_STOP=1 -q \
  -c "ALTER ROLE $(TEST_DATABASE_API_LOGIN) SET lock_timeout = '2000ms'" \
  -c "ALTER ROLE $(TEST_DATABASE_API_LOGIN) SET statement_timeout = '5000ms'" \
  >/dev/null || { echo "✗ [infra] API login bounds failed"; exit 1; }; \
echo "✓ [infra] API login $(TEST_DATABASE_API_LOGIN) holds api_runtime, under afd_db::login's bounds"
endef

_migrate-test-db: $(TEST_STATE_DEP)
	@echo "→ [infra] Applying migrations once, for the whole lane..."; \
	cd $(RUSTD_DIR) && DATABASE_URL_MIGRATOR="$(TEST_DATABASE_URL)" \
	  cargo run --locked --quiet --bin agentsfleetd -- migrate \
	  || { echo "✗ [infra] migrate failed"; exit 1; }
	@echo "✓ [infra] Schema applied"
	@$(_api_login)

# Integration tests are `#[ignore]`d and run only here, with `--ignored`;
# test-unit-rustd still compiles and lints them. afd_bench is excluded: its
# sampled counters are measurements, not assertions.
#
# $(call _rust_lane,<tally>,<label>,<command>) closes two false passes:
# `bash -o pipefail` keeps cargo's status through `tee` (/bin/sh is dash in CI),
# and zero passed tests fails, since a selection matching nothing exits 0.
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

# Modules that need the cluster to themselves (`integration_bus_exclusive` stops
# a NATS node), run apart; both invocations share this one filter.
EXCLUSIVE_FILTER := integration_bus_exclusive
EXCLUSIVE_SKIP := $(addprefix --skip ,$(EXCLUSIVE_FILTER))

test-integration-rustd: $(TEST_STATE_DEP) _migrate-test-db  ## Run the Rust substrate integration suite against compose Postgres + NATS
	@command -v cargo >/dev/null 2>&1 || { echo "✗ cargo not found. Install via: mise install rust"; exit 1; }
	@cd $(RUSTD_DIR) && cargo build --locked --all-features -p agentsfleet_runner --bin agentsfleet-recovery-fixture
	@echo "→ [rustd] Running the Rust integration suite against $(TEST_DATABASE_URL)..."; \
	$(call _rust_lane,rustd-integration.log,[rustd] integration suite,cargo test --locked --workspace --exclude afd_bench --all-features --test "*" --no-fail-fast -- --ignored $(EXCLUSIVE_SKIP))
	@echo "→ [rustd] Running the tests that need the cluster to themselves..."; \
	$(call _rust_lane,rustd-integration-exclusive.log,[rustd] integration suite (exclusive),cargo test --locked --workspace --exclude afd_bench --all-features --test "*" --no-fail-fast -- --ignored --test-threads=1 $(EXCLUSIVE_FILTER))

# A ratchet toward 100%: raise it when a run beats it, lower it only on Indy's
# word, recorded here. 98 since Nov 08, 2026: "if it fails again move the
# coverage down to 98 on the safer side and push". The judge compares exact
# decimals, so a fraction is enforced, not rounded.
RUSTD_COVERAGE_FLOOR ?= 98

# Out of the denominator, on Indy's calls: whole-file test_util modules (fixture
# code; a filename filter cannot see in-file `#[cfg]` helpers, which still count)
# and afd_bench, a measuring instrument nothing ships. codecov.yml ignores the
# same paths. afd_bench's tests still run.
RUSTD_COVERAGE_IGNORE ?= (/src/test_util(/mod)?\.rs$$|/crates/daemon/afd_bench/)

# A shard reports only the tests it ran, so scripts/rustd_coverage.py merges the
# shards' lcov files and grades the union once; a local unsharded run uses the
# same judge. It also grades the patch, the lines this branch adds, against
# codecov.yml's `rust-afd` target; an empty base skips that.
# 95 since Nov 10, 2026, on Indy's "to 95%".
RUSTD_PATCH_FLOOR ?= 95
RUSTD_PATCH_BASE ?= $(shell git merge-base HEAD origin/main 2>/dev/null)
_RUSTD_COVERAGE_JUDGE = PYTHONDONTWRITEBYTECODE=1 python3 scripts/rustd_coverage.py --floor $(RUSTD_COVERAGE_FLOOR) \
  $(if $(RUSTD_PATCH_BASE),--patch-floor $(RUSTD_PATCH_FLOOR) --patch-base $(RUSTD_PATCH_BASE))

# Each shard owns its Postgres and NATS; test-coverage-rustd-merge grades them.
#   runner     runner crates, plus the daemon suite's `integration_rust_runner::`
#   daemon     agentsfleetd minus the runner and native-stream modules
#   stream     the daemon suite's native-stream scenarios
#   api        afd_api and afd_fleet
#   substrate  everything else, by exclusion, so a new crate lands here
# No test falls between shards, and a filter matching nothing fails the lane.
# RUSTD_SHARD names one shard, which writes lcov-<shard>.info and grades
# nothing; unset runs every shard and grades. Any other subset is refused.
RUSTD_RUNNER_PACKAGES := afr_agent afr_executor afr_sandbox afr_supervisor agentsfleet_runner
RUSTD_DAEMON_PACKAGES := agentsfleetd
RUSTD_API_PACKAGES := afd_api afd_fleet
RUSTD_RUNNER_IN_DAEMON := integration_rust_runner::
RUSTD_STREAM_IN_DAEMON := integration_runner_stream::
RUSTD_SHARDS := runner daemon stream api substrate
RUSTD_SHARD ?= $(RUSTD_SHARDS)
# Plus the kernel lane's report (test-coverage-runner-kernel): not a shard,
# because it needs root and a real kernel.
RUSTD_COVERAGE_REPORTS := $(RUSTD_SHARDS) kernel

_RUSTD_COVER := cargo llvm-cov --locked --no-report --all-features
_rustd_packages = $(foreach package,$(1),-p $(package))
_RUSTD_SUBSTRATE := --workspace $(foreach package,afd_bench $(RUSTD_RUNNER_PACKAGES) $(RUSTD_DAEMON_PACKAGES) $(RUSTD_API_PACKAGES),--exclude $(package))
_RUSTD_SHARD_UNKNOWN := $(filter-out $(RUSTD_SHARDS),$(RUSTD_SHARD))
_RUSTD_SHARD_ALL := $(if $(filter-out $(RUSTD_SHARD),$(RUSTD_SHARDS)),,yes)
_RUSTD_SHARD_ONE := $(if $(filter 1,$(words $(RUSTD_SHARD))),yes,)
_RUSTD_SHARD_LCOV := $(RUSTD_DIR)/lcov-$(RUSTD_SHARD).info

# `--include-ignored` puts the instrument where the datastores are; every test
# still runs once. Migrating through `cargo llvm-cov run --no-report` measures
# the migrator too. `--no-report` is accumulate mode, so the explicit `clean`
# runs once, first; cargo-llvm-cov refuses `--no-clean` beside it.
test-coverage-rustd: $(TEST_STATE_DEP)  ## Run both Rust test tiers under coverage against live datastores (RUSTD_SHARD=<one> for a Continuous Integration shard)
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { echo "✗ cargo-llvm-cov not found. Install via: cargo install cargo-llvm-cov"; exit 1; }
	@$(if $(_RUSTD_SHARD_UNKNOWN),echo "✗ [rustd] unknown RUSTD_SHARD: $(_RUSTD_SHARD_UNKNOWN) (known: $(RUSTD_SHARDS))"; exit 1,true)
	@$(if $(or $(_RUSTD_SHARD_ALL),$(_RUSTD_SHARD_ONE)),true,echo "✗ [rustd] RUSTD_SHARD is one shard or unset; '$(RUSTD_SHARD)' would grade part of the lane"; exit 1)
	@echo "→ [rustd] Removing stale instrumented workspace artifacts..."; \
	cd $(RUSTD_DIR) && cargo llvm-cov clean --locked --workspace
	@echo "→ [infra] Applying migrations through the instrumented daemon..."; \
	started=$$(date +%s); \
	cd $(RUSTD_DIR) && DATABASE_URL_MIGRATOR="$(TEST_DATABASE_URL)" \
	  cargo llvm-cov run --locked --all-features --no-report --bin agentsfleetd -- migrate \
	  || { echo "✗ [infra] instrumented migrate failed"; exit 1; }; \
	echo "✓ [infra] Instrumented schema applied ($$(( $$(date +%s) - started ))s, its build included)"
	@$(_api_login)
ifneq ($(filter runner,$(RUSTD_SHARD)),)
	@echo "→ [rustd] Shard runner: the runner crates, then the runner against the daemon..."; \
	$(call _rust_lane,rustd-coverage-runner.log,[rustd] coverage: runner crates,$(_RUSTD_COVER) $(call _rustd_packages,$(RUSTD_RUNNER_PACKAGES)) -- --include-ignored); \
	$(call _rust_lane,rustd-coverage-runner-daemon.log,[rustd] coverage: runner against the daemon,$(_RUSTD_COVER) -p agentsfleetd --test daemon_suite -- --include-ignored $(RUSTD_RUNNER_IN_DAEMON))
endif
ifneq ($(filter daemon,$(RUSTD_SHARD)),)
	@cd $(RUSTD_DIR) && cargo llvm-cov run --locked --all-features --no-report \
	  -p agentsfleet_runner --bin agentsfleet-recovery-fixture -- --help
	@echo "→ [rustd] Shard daemon: agentsfleetd without runner and native-stream modules..."; \
	$(call _rust_lane,rustd-coverage-daemon.log,[rustd] coverage: daemon,$(_RUSTD_COVER) $(call _rustd_packages,$(RUSTD_DAEMON_PACKAGES)) -- --include-ignored --skip $(RUSTD_RUNNER_IN_DAEMON) --skip $(RUSTD_STREAM_IN_DAEMON))
endif
ifneq ($(filter stream,$(RUSTD_SHARD)),)
	@echo "→ [rustd] Shard stream: native runner sessions against the daemon..."; \
	$(call _rust_lane,rustd-coverage-stream.log,[rustd] coverage: native runner streams,$(_RUSTD_COVER) -p agentsfleetd --test daemon_suite -- --include-ignored $(RUSTD_STREAM_IN_DAEMON))
endif
ifneq ($(filter api,$(RUSTD_SHARD)),)
	@echo "→ [rustd] Shard api: the API and fleet suites..."; \
	$(call _rust_lane,rustd-coverage-api.log,[rustd] coverage: API and fleet,$(_RUSTD_COVER) $(call _rustd_packages,$(RUSTD_API_PACKAGES)) -- --include-ignored)
endif
ifneq ($(filter substrate,$(RUSTD_SHARD)),)
	@echo "→ [rustd] Shard substrate: the remaining crates, then the suites that need the cluster alone..."; \
	$(call _rust_lane,rustd-coverage-substrate.log,[rustd] coverage: substrate,$(_RUSTD_COVER) $(_RUSTD_SUBSTRATE) -- --include-ignored $(EXCLUSIVE_SKIP)); \
	$(call _rust_lane,rustd-coverage-substrate-exclusive.log,[rustd] coverage: substrate (exclusive),$(_RUSTD_COVER) $(_RUSTD_SUBSTRATE) -- --include-ignored --test-threads=1 $(EXCLUSIVE_FILTER))
endif
ifeq ($(_RUSTD_SHARD_ALL),yes)
	@echo "→ [rustd] Rendering the run's profile, then grading the floor..."; \
	cd $(RUSTD_DIR) && cargo llvm-cov report --locked --workspace \
	  --ignore-filename-regex '$(RUSTD_COVERAGE_IGNORE)' --lcov --output-path lcov-all.info \
	  || { echo "✗ [rustd] lcov report failed"; exit 1; }
	@$(_RUSTD_COVERAGE_JUDGE) --out $(RUSTD_DIR)/lcov.info $(RUSTD_DIR)/lcov-all.info
else
	@echo "→ [rustd] Rendering shard $(RUSTD_SHARD)'s profile; test-coverage-rustd-merge grades the floor..."; \
	started=$$(date +%s); \
	cd $(RUSTD_DIR) && cargo llvm-cov report --locked --workspace \
	  --ignore-filename-regex '$(RUSTD_COVERAGE_IGNORE)' --lcov --output-path lcov-$(RUSTD_SHARD).info \
	  || { echo "✗ [rustd] lcov report failed"; exit 1; }; \
	echo "✓ [rustd] shard $(RUSTD_SHARD)'s profile rendered ($$(( $$(date +%s) - started ))s)"
	@git rev-parse HEAD > $(RUSTD_DIR)/lcov-$(RUSTD_SHARD).rev
	@echo "✓ [rustd] shard $(RUSTD_SHARD) measured: $(_RUSTD_SHARD_LCOV)"
endif

# The floor over every shard's report, for CI's merge job; a missing shard, or
# one measured at another commit, is refused.
test-coverage-rustd-merge:  ## Grade the Rust line floor once over every shard's lcov report
	@$(_RUSTD_COVERAGE_JUDGE) --revision "$$(git rev-parse HEAD)" --out $(RUSTD_DIR)/lcov.info \
	  $(foreach report,$(RUSTD_COVERAGE_REPORTS),$(RUSTD_DIR)/lcov-$(report).info)

# The shard names, for the workflow's matrix: the list lives here once.
rustd-coverage-shards:  ## Print the Rust coverage lane's shard names
	@echo '$(RUSTD_SHARDS)'
