# =============================================================================
# DRY — Playwright page-render lanes (website + app), no Clerk auth
# =============================================================================

.PHONY: dry dry-smoke dry-app dry-app-smoke _dry_website _dry_website_smoke

_dry_website:  ## Internal: run website Playwright dry suite (page render, no login)
	@echo "→ [website] Running Playwright dry pass..."
	@cd ui/packages/website && bun run test:e2e
	@echo "✓ [website] Dry pass passed"

_dry_website_smoke:  ## Internal: run website Playwright dry smoke
	@echo "→ [website] Running Playwright dry smoke..."
	@cd ui/packages/website && bun run test:e2e:smoke
	@echo "✓ [website] Dry smoke passed"

dry-app:  ## Run app dry lane — Vitest + Playwright page renders, no Clerk auth
	@echo "→ [app] Running dry lane (no login)..."
	@cd ui/packages/app && bun run qa
	@echo "✓ [app] Dry lane passed"

dry-app-smoke:  ## Run app dry smoke lane — fast Vitest + Playwright smoke, no Clerk auth
	@echo "→ [app] Running dry smoke lane (no login)..."
	@cd ui/packages/app && bun run qa:smoke
	@echo "✓ [app] Dry smoke lane passed"

dry: _dry_website dry-app  ## Run dry lanes — website + app Playwright page renders (no Clerk auth)
	@echo "✓ All dry lanes passed"

dry-smoke: _dry_website_smoke dry-app-smoke  ## Run smoke dry lanes — fast website + app, no Clerk auth
	@echo "✓ All dry smoke lanes passed"
