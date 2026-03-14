# =============================================================================
# QUALITY — code quality, formatting, analysis
# =============================================================================

.PHONY: lint lint-zig lint-website lint-apps doctor _fmt _fmt_check _website_lint _app_lint _zombiectl_lint

_fmt:
	@echo "→ [zombied] Formatting Zig code..."
	@find src -name '*.zig' -exec zig fmt {} \;

_fmt_check:
	@echo "→ [zombied] Checking Zig formatting..."
	@find src -name '*.zig' -exec zig fmt --check {} \;

_website_lint:
	@echo "→ [website] Running ESLint + TypeScript check..."
	@cd ui/packages/website && bun run lint
	@cd ui/packages/website && bun run typecheck
	@echo "✓ [website] Lint passed"

_app_lint:
	@echo "→ [app] Running ESLint + TypeScript check..."
	@cd ui/packages/app && bunx next lint .
	@cd ui/packages/app && bun run typecheck
	@echo "✓ [app] Lint passed"

_zombiectl_lint:
	@echo "→ [zombiectl] Checking CLI syntax..."
	@cd zombiectl && node --check src/cli.js && node --check bin/zombiectl.js
	@echo "✓ [zombiectl] Lint passed"

lint-zig: _fmt_check _fmt  ## Lint zombied only (Zig fmt)
	@echo "✓ [zombied] Lint passed"

lint-website: _website_lint  ## Lint website only (ESLint + tsc)

lint-apps: _app_lint _zombiectl_lint  ## Lint app and zombiectl (Next.js ESLint + tsc, CLI syntax)

lint: lint-zig lint-website lint-apps  ## Lint everything (zombied + website + app + zombiectl)
	@echo "✓ All lint checks passed"

doctor:  ## Run zombied doctor (connectivity + config check)
	@zig build run -- doctor
