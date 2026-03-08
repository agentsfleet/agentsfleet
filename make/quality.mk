# =============================================================================
# QUALITY — code quality, formatting, analysis
# =============================================================================

.PHONY: lint lint-zig lint-website doctor _fmt _fmt_check _website_lint

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

lint-zig: _fmt_check _fmt  ## Lint zombied only (Zig fmt)
	@echo "✓ [zombied] Lint passed"

lint-website: _website_lint  ## Lint website only (ESLint + tsc)

lint: lint-zig lint-website  ## Lint everything (zombied + website)
	@echo "✓ All lint checks passed"

doctor:  ## Run zombied doctor (connectivity + config check)
	@zig build run -- doctor
