# =============================================================================
# ACCEPTANCE — dashboard + CLI live-API acceptance e2e (local twins of CI)
# =============================================================================

.PHONY: acceptance-build acceptance-e2e acceptance-execution acceptance-cli
.PHONY: acceptance-local

acceptance-local:  ## Build and start local services, then run acceptance (SUITE=e2e|cli|execution; default: both).
	@bash deploy/local/acceptance.sh "$(SUITE)"

# Local twins of the pipeline acceptance jobs — same command, env-driven target
# (local / vercel.app / api-dev / api), so a developer runs exactly what CI runs.
# Run both with: make acceptance-e2e acceptance-cli
# `acceptance-execution` is one journey out of acceptance-e2e — the fleet that
# executes to a real result — for the inner loop while that walk is being fixed.

acceptance-build:  ## Build the daemon, runner, CLI and dashboard for acceptance.
	@bun scripts/build-acceptance.ts

acceptance-e2e:  ## Dashboard auth acceptance — Clerk sign-in + install + lifecycle (Playwright vs live API). Mirrors CI acceptance-e2e.
	@echo "→ [app] Running dashboard acceptance e2e (Clerk sign-in + lifecycle)..."
	@cd ui/packages/app && bun run test:e2e:acceptance
	@echo "✓ [app] dashboard acceptance e2e passed"

acceptance-execution:  ## The fleet-execution journey alone (install → lease → execute → observe vs live API). A slice of acceptance-e2e, never its replacement.
	@echo "→ [app] Running the fleet-execution journey..."
	@cd ui/packages/app && bun run test:e2e:acceptance:execution
	@echo "✓ [app] fleet-execution journey passed"

acceptance-cli:  ## CLI auth acceptance — agentsfleet login + token lifecycle vs live API. Mirrors CI acceptance-cli.
	@echo "→ [agentsfleet] Running CLI acceptance e2e (login + token lifecycle)..."
	@cd cli && bun run test:acceptance:deterministic:run && bun run test:acceptance:live:run
	@echo "✓ [agentsfleet] CLI acceptance e2e passed"
