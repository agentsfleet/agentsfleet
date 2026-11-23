.PHONY: dry dry-app dry-app-smoke dry-website dry-website-smoke

dry-website:  ## Run the website dry lane — Playwright page renders, no login
	@echo "→ [website] Running Playwright dry pass..."
	@cd ui/packages/website && bun run test:e2e
	@echo "✓ [website] Dry pass passed"

dry-website-smoke:  ## Run the website dry smoke — Playwright smoke spec, no login
	@echo "→ [website] Running Playwright dry smoke..."
	@cd ui/packages/website && bun run test:e2e:smoke
	@echo "✓ [website] Dry smoke passed"

dry-app:  ## Run app dry lane — Vitest + Playwright page renders, no Clerk auth
	@echo "→ [app] Running dry lane (no login)..."
	@cd ui/packages/app && bun run qa
	@echo "✓ [app] Dry lane passed"

dry-app-smoke:  ## Run app dry smoke lane — Playwright smoke spec, no Clerk auth
	@echo "→ [app] Running dry smoke lane (no login)..."
	@cd ui/packages/app && bun run qa:smoke
	@echo "✓ [app] Dry smoke lane passed"

dry: dry-website dry-app  ## Run dry lanes — website + app Playwright page renders (no Clerk auth)
	@echo "✓ All dry lanes passed"
