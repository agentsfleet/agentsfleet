# =============================================================================
# TEST-UNIT — agentsfleetd, agentsfleet, website, app + multi-package coverage gate
# =============================================================================

.PHONY: test-unit-rustd test-unit-rustd-runner test-unit-rustd-daemon test-unit-rustd-daemon-libs test-unit-cli test-unit-website test-unit-app test-unit-design-system test-coverage-all test-runner-kernel test-runner-startup test-coverage-runner-kernel

# Three shards, one per crate family, so Continuous Integration runs each on its
# own runner at once and `test-unit-rustd` is all three in order: the whole
# workspace, through the invocations Continuous Integration makes.
#
#   runner       `afr_*` and `agentsfleet_runner`: the Rust runner
#   daemon       `agentsfleetd`: the daemon binary and its suites
#   daemon-libs  every other crate: `afd_*`, `afd_bench` included
#
# Families by crate-name prefix rather than a list, so a crate added under
# `crates/` joins its family's shard with no edit here; the directory name is
# the crate name (`rustd/Cargo.toml`, M-CRATES-FLAT-FOLDER). `daemon-libs` is
# the rest of the workspace rather than a prefix, so a crate no family names
# still runs. `afd_bench` is in `daemon-libs`: the coverage lane drops its
# LINES as a measuring instrument, but its tests run here, as they always have.
#
# Every selection runs through `_rust_lane`, whose zero-tests guard fails a
# shard that selects nothing.
_rustd_family = $(sort $(notdir $(wildcard $(RUSTD_DIR)/crates/$(1)*)))
_RUSTD_UNIT_RUNNER = $(call _rustd_family,afr_) agentsfleet_runner
_RUSTD_UNIT_DAEMON = agentsfleetd
_RUSTD_UNIT_DAEMON_LIBS = $(filter-out $(_RUSTD_UNIT_RUNNER) $(_RUSTD_UNIT_DAEMON),$(call _rustd_family,))
# --all-features for the same reason lint-rustd carries it: the `test-util`
# mocks are how the failure paths a real datastore will not produce on demand
# get reached at all, and a default-feature run silently skips them.
_RUSTD_UNIT_TEST = $(RUST_CARGO) test --all-features
_RUSTD_NEED_CARGO = command -v cargo >/dev/null 2>&1 || { echo "✗ cargo not found. Install via: mise install rust"; exit 1; }

test-unit-rustd: test-unit-rustd-runner test-unit-rustd-daemon test-unit-rustd-daemon-libs  ## Run the Rust workspace unit tests (cargo), shard by shard

test-unit-rustd-runner:  ## Rust unit shard: the runner's crates (afr_*, agentsfleet_runner)
	@$(_RUSTD_NEED_CARGO)
	@$(call _rust_lane,rustd-unit-runner.log,[rustd] unit: runner,$(_RUSTD_UNIT_TEST) $(call _rustd_packages,$(_RUSTD_UNIT_RUNNER)))

test-unit-rustd-daemon:  ## Rust unit shard: agentsfleetd, the daemon binary and its suites
	@$(_RUSTD_NEED_CARGO)
	@$(call _rust_lane,rustd-unit-daemon.log,[rustd] unit: daemon,$(_RUSTD_UNIT_TEST) $(call _rustd_packages,$(_RUSTD_UNIT_DAEMON)))

test-unit-rustd-daemon-libs:  ## Rust unit shard: the daemon's library crates (afd_*)
	@PYTHONDONTWRITEBYTECODE=1 python3 scripts/cargo_local_test.py
	@bash scripts/local-dev-test.sh
	@$(_RUSTD_NEED_CARGO)
	@$(call _rust_lane,rustd-unit-daemon-libs.log,[rustd] unit: daemon libraries,$(_RUSTD_UNIT_TEST) $(call _rustd_packages,$(_RUSTD_UNIT_DAEMON_LIBS)))

test-unit-cli:  ## Run agentsfleet CLI unit tests (bun)
	@echo "→ [agentsfleet] Building dist/ (tests spawn dist/bin/agentsfleet.js)..."
	@cd cli && bun run build >/dev/null
	@echo "→ [agentsfleet] Running Bun unit tests..."
	@# --timeout 30000: the help-e2e / PTY tests spawn the built binary and wait
	@# for output; bun's 5s default flakes under the parallel pre-push lane load.
	@cd cli && bun test --timeout 30000
	@echo "✓ [agentsfleet] Unit tests passed"

test-unit-website:  ## Run website unit tests (vitest)
	@echo "→ [website] Running Vitest unit tests..."
	@cd ui/packages/website && bun run test
	@echo "✓ [website] Unit tests passed"

test-unit-app:  ## Run app unit tests (vitest, no coverage)
	@echo "→ [app] Running Vitest unit tests..."
	@cd ui/packages/app && bun run test
	@echo "✓ [app] Unit tests passed"

test-unit-design-system:  ## Run design-system unit tests (vitest, no coverage)
	@echo "→ [design-system] Running Vitest unit tests..."
	@cd ui/packages/design-system && bun run test
	@echo "✓ [design-system] Unit tests passed"

test-coverage-all:  ## Run coverage gates across app, website, agentsfleet, and design-system
	@echo "→ [app] Running Vitest with --coverage..."
	@cd ui/packages/app && bun run test:coverage
	@echo "→ [website] Running Vitest with --coverage..."
	@cd ui/packages/website && bun run test:coverage
	@echo "→ [agentsfleet] Enforcing the 100% coverage floor (scripts/enforce-coverage.mjs)..."
	@cd cli && bun run test
	@echo "→ [design-system] Running Vitest with --coverage..."
	@cd ui/packages/design-system && bun run test:coverage
	@echo "✓ All package coverage gates passed"

# The Rust runner's sandbox, proven on a real kernel: capabilities, seccomp,
# Landlock, the workspace disk, cgroup limits, no network, the toolbox, warm
# starts. It needs Linux, root, bubblewrap, Landlock, EROFS and cgroup v2, and
# it FAILS naming what is missing rather than skipping — a lane that skips
# passes without proving anything. Cargo builds as the invoking user; only the
# lane binary runs as root, through cargo's runner. KERNEL_LANE_RUNNER is that
# elevation (empty when already root, as in a container). Cargo's runner must
# still name a command, so an already-root invocation uses env.
# The runner is set through cargo's per-host variable rather than `--config`.
KERNEL_LANE_RUNNER ?= sudo -E
# No variables are required for the full lane. AFR_TOOLBOX_IMAGE optionally
# reuses an existing image; the default builds it from the pinned manifest.
KERNEL_LANE_HOST = $(shell cd $(RUSTD_DIR) && rustc -vV | sed -n 's/^host: //p' | tr 'a-z-' 'A-Z_')

test-runner-kernel:  ## Prove the Rust runner's sandbox on a real Linux kernel (root, bubblewrap, Landlock, cgroup v2); fails, never skips
	@$(KERNEL_LANE_RUNNER) bash scripts/toolbox/test_measure_first_use.sh
	@image="$${AFR_TOOLBOX_IMAGE:-}"; \
	  if [ -z "$$image" ]; then image="$$($(KERNEL_LANE_RUNNER) bash scripts/toolbox/build.sh "$(TOOLBOX_DIR)")" || exit $$?; fi; \
	  test -f "$$image" && image="$$(realpath "$$image")" && \
	  cd $(RUSTD_DIR) && AFR_TOOLBOX_IMAGE="$$image" \
	  CARGO_TARGET_$(KERNEL_LANE_HOST)_RUNNER="$(or $(KERNEL_LANE_RUNNER),env)" \
	  $(RUST_CARGO) run -p afr_sandbox --features test-util --example kernel_lane
	@$(MAKE) --no-print-directory test-runner-startup

# Uses the deployed unit and the shipping binary, including its release key.
# An absent toolbox must refuse admission after successful host preparation.
# The kernel lane above separately proves signed admission and child limits.
test-runner-startup:  ## Prove default/explicit engine startup under systemd on a Linux host
	@cd $(RUSTD_DIR) && $(RUST_CARGO) build -p agentsfleet_runner --bin agentsfleet-runner
	@target="$$(cd $(RUSTD_DIR) && $(RUST_CARGO) metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')" && \
	  $(KERNEL_LANE_RUNNER) bash scripts/test-runner-startup.sh "$$target/debug/agentsfleet-runner" deploy/baremetal/agentsfleet-runner.service

# The kernel lane under coverage: both the example and the systemd binary
# share the instrumented environment, producing a report beside the integration lane's
# shards. `test-coverage-rustd-merge` grades it with them, so the Linux-only
# lines only this lane reaches count toward the floor. Its caller is the
# kernel job in .github/workflows/test-integration-rustd.yml.
test-coverage-runner-kernel:  ## Measure the kernel lane under coverage into rustd/lcov-kernel.info for test-coverage-rustd-merge
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { echo "✗ cargo-llvm-cov not found. Install via: cargo install cargo-llvm-cov"; exit 1; }
	@set -e; cd $(RUSTD_DIR); \
	  coverage_env="$$($(RUST_CARGO) llvm-cov show-env --sh)"; eval "$$coverage_env"; \
	  $(RUST_CARGO) llvm-cov clean --workspace; \
	  cd ..; $(MAKE) --no-print-directory test-runner-kernel; \
	  cd $(RUSTD_DIR); $(RUST_CARGO) llvm-cov report --workspace \
	  --ignore-filename-regex '$(RUSTD_COVERAGE_IGNORE)' --lcov --output-path lcov-kernel.info \
	  || { echo "✗ [rustd] kernel lane lcov report failed"; exit 1; }
	@git rev-parse HEAD > $(RUSTD_DIR)/lcov-kernel.rev
	@echo "✓ [rustd] kernel lane measured: $(RUSTD_DIR)/lcov-kernel.info"
