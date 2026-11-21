.PHONY: test-unit-rustd test-unit-rustd-runner test-unit-rustd-daemon test-unit-rustd-daemon-libs build-test-unit-rustd test-coverage-cli test-coverage-website test-coverage-app test-coverage-design-system test-coverage-all test-runner-kernel test-runner-startup test-coverage-runner-kernel

# Three shards, which CI runs at once; test-unit-rustd runs all three in order.
# A shard is a list of folder globs under rustd/, never of crate names, so a
# crate joins the shard of the folder it sits in with no edit here, and a move
# changes its shard rather than dropping it. af_e2e's tests/layers.rs proves
# every member lands in exactly one. `_rust_lane` fails a shard that selects
# nothing.
RUSTD_UNIT_RUNNER_DIRS := crates/harness/* crates/runner/* guffbolt
RUSTD_UNIT_DAEMON_DIRS := crates/daemon/main crates/e2e/*
RUSTD_UNIT_DAEMON_LIBS_DIRS := crates/foundation/* crates/protocol/* crates/daemon/afd_*
_rustd_manifest_name = $(shell awk -F'"' '/^name = /{print $$2; exit}' $(1))
_rustd_dirs = $(sort $(foreach manifest,$(wildcard $(addprefix $(RUSTD_DIR)/,$(addsuffix /Cargo.toml,$(1)))),$(call _rustd_manifest_name,$(manifest))))
_RUSTD_UNIT_RUNNER = $(call _rustd_dirs,$(RUSTD_UNIT_RUNNER_DIRS))
_RUSTD_UNIT_DAEMON = $(call _rustd_dirs,$(RUSTD_UNIT_DAEMON_DIRS))
_RUSTD_UNIT_DAEMON_LIBS = $(call _rustd_dirs,$(RUSTD_UNIT_DAEMON_LIBS_DIRS))
# --all-features: the `test-util` mocks reach failure paths a real datastore
# will not produce on demand.
# --no-fail-fast: one run names every failing test binary, not only the first.
_RUSTD_UNIT_TEST = cargo test --locked --all-features --no-fail-fast
_RUSTD_NEED_CARGO = command -v cargo >/dev/null 2>&1 || { echo "✗ cargo not found. Install via: mise install rust"; exit 1; }

test-unit-rustd: test-unit-rustd-runner test-unit-rustd-daemon test-unit-rustd-daemon-libs  ## Run the Rust workspace unit tests (cargo), shard by shard

test-unit-rustd-runner:  ## Rust unit shard: the harness/ and runner/ layers, and guffbolt
	@$(_RUSTD_NEED_CARGO)
	@$(call _rust_lane,rustd-unit-runner.log,[rustd] unit: runner,$(_RUSTD_UNIT_TEST) $(call _rustd_packages,$(_RUSTD_UNIT_RUNNER)))

test-unit-rustd-daemon:  ## Rust unit shard: agentsfleetd, the daemon binary, and the e2e/ layer
	@$(_RUSTD_NEED_CARGO)
	@$(call _rust_lane,rustd-unit-daemon.log,[rustd] unit: daemon,$(_RUSTD_UNIT_TEST) $(call _rustd_packages,$(_RUSTD_UNIT_DAEMON)))

test-unit-rustd-daemon-libs:  ## Rust unit shard: the foundation/ and protocol/ layers, and the daemon/ libraries
	@bash scripts/local-dev-test.sh
	@bun test deploy/local/bootstrap.test.ts
	@bash deploy/local/linux_test.sh
	@$(_RUSTD_NEED_CARGO)
	@$(call _rust_lane,rustd-unit-daemon-libs.log,[rustd] unit: daemon libraries,$(_RUSTD_UNIT_TEST) $(call _rustd_packages,$(_RUSTD_UNIT_DAEMON_LIBS)))

# Cache warm-up for warm-cache.yml: the shards' selections, compiled once with --no-run.
build-test-unit-rustd:  ## Compile the Rust unit shards' test binaries without running them (main-branch cache warm-up)
	@$(_RUSTD_NEED_CARGO)
	@cd $(RUSTD_DIR) && $(_RUSTD_UNIT_TEST) --no-run $(call _rustd_packages,$(_RUSTD_UNIT_RUNNER))
	@cd $(RUSTD_DIR) && $(_RUSTD_UNIT_TEST) --no-run $(call _rustd_packages,$(_RUSTD_UNIT_DAEMON))
	@cd $(RUSTD_DIR) && $(_RUSTD_UNIT_TEST) --no-run $(call _rustd_packages,$(_RUSTD_UNIT_DAEMON_LIBS))

# Each is the exact command its CI job (.github/workflows/test.yml) and pre-push run.
test-coverage-cli:  ## agentsfleet CLI unit tests with the 100% coverage floor (scripts/enforce-coverage.mjs)
	@echo "→ [agentsfleet] Enforcing the 100% coverage floor (scripts/enforce-coverage.mjs)..."
	@cd cli && bun run test

test-coverage-website:  ## Website unit tests with coverage (vitest --coverage)
	@echo "→ [website] Running Vitest with --coverage..."
	@cd ui/packages/website && bun run test:coverage

test-coverage-app:  ## App unit tests with coverage (vitest --coverage)
	@echo "→ [app] Running Vitest with --coverage..."
	@cd ui/packages/app && bun run test:coverage

test-coverage-design-system:  ## Design-system unit tests with coverage (vitest --coverage)
	@echo "→ [design-system] Running Vitest with --coverage..."
	@cd ui/packages/design-system && bun run test:coverage

# Called in sequence rather than listed as prerequisites, so `make -j` cannot
# run four coverage suites at once; the first red stops the rest.
test-coverage-all:  ## Run coverage gates across app, website, agentsfleet, and design-system
	@$(MAKE) --no-print-directory test-coverage-app
	@$(MAKE) --no-print-directory test-coverage-website
	@$(MAKE) --no-print-directory test-coverage-cli
	@$(MAKE) --no-print-directory test-coverage-design-system
	@echo "✓ All package coverage gates passed"

# The runner's sandbox on a real kernel. It FAILS naming what is missing rather
# than skipping. Cargo builds as the invoking user; only the lane binary runs as
# root, through cargo's per-host runner variable. An empty KERNEL_LANE_RUNNER
# (already root) falls back to `env`, because the runner must name a command.
KERNEL_LANE_RUNNER ?= sudo -E
# AFR_TOOLBOX_IMAGE reuses an image; unset, the lane builds one from the manifest.
KERNEL_LANE_HOST = $(shell cd $(RUSTD_DIR) && rustc -vV | sed -n 's/^host: //p' | tr 'a-z-' 'A-Z_')

test-runner-kernel:  ## Prove the Rust runner's sandbox on a real Linux kernel (root, bubblewrap, Landlock, cgroup v2); fails, never skips
	@$(KERNEL_LANE_RUNNER) bash playbooks/operations/acceptance/test_toolbox_first_use.sh
	@image="$${AFR_TOOLBOX_IMAGE:-}"; \
	  if [ -z "$$image" ]; then image="$$($(KERNEL_LANE_RUNNER) bash scripts/toolbox/build.sh "$(TOOLBOX_DIR)")" || exit $$?; fi; \
	  test -f "$$image" && image="$$(realpath "$$image")" && \
	  cd $(RUSTD_DIR) && AFR_TOOLBOX_IMAGE="$$image" AFR_TOOLBOX_SOURCE="$(CURDIR)/scripts/toolbox" \
	  CARGO_TARGET_$(KERNEL_LANE_HOST)_RUNNER="$(or $(KERNEL_LANE_RUNNER),env)" \
	  cargo run --locked -p agentsfleet_runner --features test-util --example kernel_lane
	@$(MAKE) --no-print-directory test-runner-startup

# The deployed unit and the shipping binary: an absent toolbox must refuse
# admission after host preparation.
test-runner-startup:  ## Prove default/explicit engine startup under systemd on a Linux host
	@cd $(RUSTD_DIR) && cargo build --locked -p agentsfleet_runner --bin agentsfleet-runner
	@target="$$(cd $(RUSTD_DIR) && cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')" && \
	  $(KERNEL_LANE_RUNNER) bash scripts/test-runner-startup.sh "$$target/debug/agentsfleet-runner" deploy/baremetal/agentsfleet-runner.service

# The kernel lane under coverage, so the Linux-only lines it reaches count;
# test-coverage-rustd-merge grades its report with the shards'.
test-coverage-runner-kernel:  ## Measure the kernel lane under coverage into rustd/lcov-kernel.info for test-coverage-rustd-merge
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { echo "✗ cargo-llvm-cov not found. Install via: cargo install cargo-llvm-cov"; exit 1; }
	@set -e; cd $(RUSTD_DIR); \
	  coverage_env="$$(cargo llvm-cov show-env --sh)"; eval "$$coverage_env"; \
	  cargo llvm-cov clean --locked --workspace; \
	  cd ..; $(MAKE) --no-print-directory test-runner-kernel; \
	  cd $(RUSTD_DIR); cargo llvm-cov report --locked --workspace \
	  --ignore-filename-regex '$(RUSTD_COVERAGE_IGNORE)' --lcov --output-path lcov-kernel.info \
	  || { echo "✗ [rustd] kernel lane lcov report failed"; exit 1; }
	@git rev-parse HEAD > $(RUSTD_DIR)/lcov-kernel.rev
	@echo "✓ [rustd] kernel lane measured: $(RUSTD_DIR)/lcov-kernel.info"
