# =============================================================================
# TEST-UNIT — agentsfleetd, agentsfleet, website, app + multi-package coverage gate
# =============================================================================

.PHONY: test-unit-rustd test-unit-runner test-unit-cli test-unit-website test-unit-app test-unit-design-system test-coverage-all

test-unit-rustd:  ## Run the Rust workspace unit tests (cargo)
	@command -v cargo >/dev/null 2>&1 || { echo "✗ cargo not found. Install via: mise install rust"; exit 1; }
	@# --all-features for the same reason lint-rustd carries it: the `test-util`
	@# mocks are how the failure paths a real datastore will not produce on
	@# demand get reached at all, and a default-feature run silently skips them.
	@cd $(RUSTD_DIR) && $(WITH_PROGRESS) "[rustd] cargo test --workspace" -- \
	  cargo test --workspace --all-features

# The Zig compiler `agentsfleet-runner` builds and ships with, spelled once in
# `build.zig.zon`. The manifest's floor admits any newer Zig, which then fails
# on standard-library drift a long way from the cause, so the tests refuse
# anything but this exact version up front. Recursive, so the read happens
# only when a runner target asks for it.
RUNNER_ZIG_VERSION = $(shell sed -n 's/^[[:space:]]*\.minimum_zig_version = "\(.*\)",/\1/p' build.zig.zon)

test-unit-runner:  ## Run the Zig runner's unit tests (zig build test, on build.zig.zon's Zig)
	@command -v zig >/dev/null 2>&1 || { echo "✗ zig not found. Install via: mise install zig@$(RUNNER_ZIG_VERSION)"; exit 1; }
	@found="$$(zig version)"; [ "$$found" = "$(RUNNER_ZIG_VERSION)" ] || { \
	  echo "✗ [runner] zig $$found found; the runner's tests need Zig $(RUNNER_ZIG_VERSION) (build.zig.zon). Install via: mise install zig@$(RUNNER_ZIG_VERSION)"; \
	  exit 1; }
	@$(WITH_PROGRESS) "[runner] zig build test" -- zig build --build-file build_runner.zig test

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
