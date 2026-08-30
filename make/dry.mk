# =============================================================================
# DRY — Playwright page-render lanes (website + app), no Clerk auth
# =============================================================================

.PHONY: dry dry-smoke dry-app dry-app-smoke dry-app-rustd _dry_website _dry_website_smoke

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

# The same page-render suite, served by the Rust daemon instead of api-dev.
#
# `playwright.config.ts` declares its backend explicitly rather than falling
# back — production code throws on an unset NEXT_PUBLIC_API_URL — and threads
# whatever it is given into the Next server it spawns. So pointing the lane at a
# different daemon is exactly one variable, and the suite it runs is the same
# one `dry-app` runs. That is the property worth having: a page that renders
# against the daemon being retired must render against the one replacing it,
# proven by the identical suite rather than a parallel copy of it.
#
# `_ensure-local-daemon` boots the stack and waits for /healthz first, so this
# is one command rather than a docker step and a make step that have to agree
# about a port.
dry-app-rustd: _ensure-local-daemon  ## Run the app dry lane against the locally-booted Rust daemon
	@echo "→ [app] Running dry lane against $(LOCAL_DAEMON_URL)..."
	@cd ui/packages/app && NEXT_PUBLIC_API_URL="$(LOCAL_DAEMON_URL)" bun run qa
	@echo "✓ [app] Dry lane passed against the Rust daemon"

dry: _dry_website dry-app  ## Run dry lanes — website + app Playwright page renders (no Clerk auth)
	@echo "✓ All dry lanes passed"

dry-smoke: _dry_website_smoke dry-app-smoke  ## Run smoke dry lanes — fast website + app, no Clerk auth
	@echo "✓ All dry smoke lanes passed"
