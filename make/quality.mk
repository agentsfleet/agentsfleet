# =============================================================================
# QUALITY — code quality, formatting, analysis
# =============================================================================

.PHONY: lint lint-zig lint-website lint-apps lint-ci doctor check-pg-drain _fmt _fmt_check _zlint_check _pg_drain_check _website_lint _app_lint _zombiectl_lint _actionlint_check

ZLINT ?= zlint
ACTIONLINT ?= actionlint

_fmt:
	@echo "→ [zombied] Formatting Zig code..."
	@find src -name '*.zig' -exec zig fmt {} \;

_fmt_check:
	@echo "→ [zombied] Checking Zig formatting..."
	@find src -name '*.zig' -exec zig fmt --check {} \;

_zlint_check:
	@echo "→ [zombied] Running ZLint..."
	@command -v $(ZLINT) >/dev/null 2>&1 || { echo "ZLint not found. Install v0.7.9 or set ZLINT=/path/to/zlint."; exit 1; }
	@$(ZLINT) --deny-warnings
	@echo "✓ [zombied] ZLint passed"

_website_lint:
	@echo "→ [website] Running ESLint + TypeScript check..."
	@cd ui/packages/website && bun run lint
	@cd ui/packages/website && bun run typecheck
	@echo "✓ [website] Lint passed"

_app_lint:
	@echo "→ [app] Running ESLint + TypeScript check..."
	@cd ui/packages/app && bun run lint
	@cd ui/packages/app && bun run typecheck
	@echo "✓ [app] Lint passed"

_zombiectl_lint:
	@echo "→ [zombiectl] Checking CLI syntax..."
	@cd zombiectl && node --check src/cli.js && node --check bin/zombiectl.js
	@echo "✓ [zombiectl] Lint passed"

_pg_drain_check:
	@echo "→ [zombied] Checking pg query drain discipline..."
	@python3 lint-zig.py src
	@echo "✓ [zombied] pg-drain check passed"

_actionlint_check:
	@echo "→ [ci] Running actionlint on GitHub Actions workflows..."
	@command -v $(ACTIONLINT) >/dev/null 2>&1 || { echo "actionlint not found. Install via: mise install actionlint"; exit 1; }
	@$(ACTIONLINT) .github/workflows/*.yml
	@echo "✓ [ci] actionlint passed"

check-pg-drain: _pg_drain_check  ## Check that all conn.query() calls have a .drain()

lint-zig: _fmt_check _zlint_check _pg_drain_check  ## Lint zombied only (Zig fmt check + ZLint + pg-drain)
	@echo "✓ [zombied] Lint passed"

lint-website: _website_lint  ## Lint website only (ESLint + tsc)

lint-apps: _app_lint _zombiectl_lint  ## Lint app and zombiectl (Next.js ESLint + tsc, CLI syntax)

lint-ci: _actionlint_check  ## Lint GitHub Actions workflows (actionlint)

lint: lint-zig lint-website lint-apps lint-ci  ## Lint everything (zombied + website + app + zombiectl + CI workflows)
	@echo "✓ All lint checks passed"

doctor:  ## Run zombied doctor (connectivity + config check)
	@zig build run -- doctor
