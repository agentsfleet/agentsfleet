# =============================================================================
# TEST-UNIT — agentsfleetd, agentsfleet, website, app + multi-package coverage gate
# =============================================================================

.PHONY: test-unit-rustd wire-fixtures test-unit-cli test-unit-website test-unit-app test-unit-design-system test-coverage-all

test-unit-rustd:  ## Run the Rust workspace unit tests (cargo)
	@echo "→ [rustd] Running cargo unit tests..."
	@command -v cargo >/dev/null 2>&1 || { echo "✗ cargo not found. Install via: mise install rust"; exit 1; }
	@# --all-features for the same reason lint-rustd carries it: the `test-util`
	@# mocks are how the failure paths a real datastore will not produce on
	@# demand get reached at all, and a default-feature run silently skips them.
	@cd $(RUSTD_DIR) && cargo test --workspace --all-features
	@echo "✓ [rustd] Unit tests passed"

# Regenerates the committed wire fixtures from the Zig source of truth. Runs as
# `zig run`, not through build.zig: every src/lib/contract import is a sibling
# path, so the emitter compiles standalone and this milestone leaves the Zig
# build graph untouched.
#
# Committed output on purpose — a Zig wire change then lands as a RED DIFF in
# samples/fixtures/wire-v2/ plus a red Rust round-trip, rather than as a silent
# skew nobody notices until a runner deserializes garbage.
wire-fixtures:  ## Regenerate samples/fixtures/wire-v2/ from src/lib/contract (Zig is the source of truth)
	@echo "→ [wire] Regenerating canonical fixtures from src/lib/contract..."
	@zig run src/lib/contract/fixture_export.zig
	@echo "✓ [wire] $$(ls samples/fixtures/wire-v2/*.json | wc -l | tr -d ' ') files written — review the diff before committing"

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

test-coverage-all:  ## Run coverage gates across Zig, app, website, agentsfleet, and design-system
	@echo "→ [app] Running Vitest with --coverage..."
	@cd ui/packages/app && bun run test:coverage
	@echo "→ [website] Running Vitest with --coverage..."
	@cd ui/packages/website && bun run test:coverage
	@echo "→ [agentsfleet] Enforcing the 100% coverage floor (scripts/enforce-coverage.mjs)..."
	@cd cli && bun run test
	@echo "→ [design-system] Running Vitest with --coverage..."
	@cd ui/packages/design-system && bun run test:coverage
	@echo "✓ All package coverage gates passed"
