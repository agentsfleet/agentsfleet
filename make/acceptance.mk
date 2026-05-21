# =============================================================================
# ACCEPTANCE — live-e2e-all full integration suite + dashboard/CLI acceptance e2e
# =============================================================================

.PHONY: live-e2e-all acceptance-e2e cli-acceptance

live-e2e-all: _test-integration-full  ## Run the FULL Zig integration suite unfiltered vs real Postgres + Redis (no -Dtest-filter)
	@echo "✓ [zombied] live-e2e-all (full integration suite, unfiltered) passed"

# Local twins of the pipeline acceptance jobs — same command, env-driven target
# (local / vercel.app / api-dev / api), so a developer runs exactly what CI runs.
# Run both with: make acceptance-e2e cli-acceptance

acceptance-e2e:  ## Dashboard auth acceptance — Clerk sign-in + install + lifecycle (Playwright vs live API). Mirrors CI acceptance-e2e-{dev,prod}.
	@echo "→ [app] Running dashboard acceptance e2e (Clerk sign-in + lifecycle)..."
	@cd ui/packages/app && bun run test:e2e:acceptance
	@echo "✓ [app] dashboard acceptance e2e passed"

cli-acceptance:  ## CLI auth acceptance — zombiectl login + token lifecycle vs live API. Mirrors CI cli-acceptance-{dev,prod}.
	@echo "→ [zombiectl] Running CLI acceptance e2e (login + token lifecycle)..."
	@cd zombiectl && bun run test:acceptance
	@echo "✓ [zombiectl] CLI acceptance e2e passed"
