# =============================================================================
# ACCEPTANCE — dashboard + CLI live-API acceptance e2e (local twins of CI)
# =============================================================================

.PHONY: acceptance-e2e acceptance-execution cli-acceptance

# Local twins of the pipeline acceptance jobs — same command, env-driven target
# (local / vercel.app / api-dev / api), so a developer runs exactly what CI runs.
# Run both with: make acceptance-e2e cli-acceptance
# `acceptance-execution` is one journey out of acceptance-e2e — the fleet that
# executes to a real result — for the inner loop while that walk is being fixed.

acceptance-e2e:  ## Dashboard auth acceptance — Clerk sign-in + install + lifecycle (Playwright vs live API). Mirrors CI acceptance-e2e-{dev,prod}.
	@echo "→ [app] Running dashboard acceptance e2e (Clerk sign-in + lifecycle)..."
	@cd ui/packages/app && bun run test:e2e:acceptance
	@echo "✓ [app] dashboard acceptance e2e passed"

acceptance-execution:  ## The fleet-execution journey alone (install → lease → execute → observe vs live API). A slice of acceptance-e2e, never its replacement.
	@echo "→ [app] Running the fleet-execution journey..."
	@cd ui/packages/app && bun run test:e2e:acceptance:execution
	@echo "✓ [app] fleet-execution journey passed"

cli-acceptance:  ## CLI auth acceptance — agentsfleet login + token lifecycle vs live API. Mirrors CI cli-acceptance-{dev,prod}.
	@echo "→ [agentsfleet] Running CLI acceptance e2e (login + token lifecycle)..."
	@cd cli && bun run test:acceptance
	@echo "✓ [agentsfleet] CLI acceptance e2e passed"
