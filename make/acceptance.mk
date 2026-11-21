.PHONY: acceptance acceptance-build acceptance-e2e acceptance-execution acceptance-cli

ACCEPTANCE_SCOPE ?= full

# The whole stack from this checkout on a Linux machine (privileged Docker);
# people start it through `orly burst`, not on a laptop.
acceptance:  ## Build and start the full stack on this Linux machine, then run acceptance (SUITE=e2e|cli|execution; default: both).
	@bash deploy/local/acceptance.sh "$(SUITE)"

# Local twins of CI's acceptance jobs: same command, target from the environment.
# `acceptance-execution` is one journey of acceptance-e2e, for the inner loop.

acceptance-build:  ## Build the daemon, runner, CLI and dashboard for acceptance.
	@bun scripts/build-acceptance.ts

acceptance-e2e:  ## Dashboard acceptance (ACCEPTANCE_SCOPE=full|post-deploy|soak-mini|ten-fleets; default: full).
	@echo "→ [app] Running dashboard acceptance e2e (Clerk sign-in + lifecycle)..."
	@case "$(ACCEPTANCE_SCOPE)" in \
	  full) set -- ;; \
	  post-deploy) set -- --project=journeys --grep=@post-deploy ;; \
	  soak-mini) set -- --project=soak --grep=@soak-mini ;; \
	  ten-fleets) set -- --project=soak --grep=@soak-marathon ;; \
	  *) printf 'Unknown ACCEPTANCE_SCOPE: %s\n' "$(ACCEPTANCE_SCOPE)" >&2; exit 2 ;; \
	esac; \
	cd ui/packages/app && AGENTSFLEET_ACCEPTANCE_SCOPE="$(ACCEPTANCE_SCOPE)" bun run test:e2e:acceptance "$$@"
	@echo "✓ [app] dashboard acceptance e2e passed"

acceptance-execution:  ## The fleet-execution journey alone (install → run → execute → observe vs live API). A slice of acceptance-e2e, never its replacement.
	@echo "→ [app] Running the fleet-execution journey..."
	@cd ui/packages/app && bun run test:e2e:acceptance:execution
	@echo "✓ [app] fleet-execution journey passed"

acceptance-cli:  ## CLI auth acceptance — agentsfleet login + token lifecycle vs live API. Mirrors CI acceptance-cli.
	@echo "→ [agentsfleet] Running CLI acceptance e2e (login + token lifecycle)..."
	@cd cli && bun run test:acceptance:deterministic:run && bun run test:acceptance:live:run
	@echo "✓ [agentsfleet] CLI acceptance e2e passed"
