# =============================================================================
# DEV — local development
# =============================================================================

# The URL is derived without starting infrastructure or reading credentials.
LOCAL_DAEMON_URL := $(shell bash scripts/local-dev.sh url)

.PHONY: up down status _ensure-local-daemon seed-models

up:  ## Reuse shared infrastructure and start this worktree's native daemon
	@bash scripts/local-dev.sh up
	@if [ "$${FOLLOW_LOGS:-1}" = "1" ]; then bash scripts/local-dev.sh logs; fi

down:  ## Stop this daemon, or remove an orphan stack with DEV_STACK=<compose project>
	@if [ -n "$(DEV_STACK)" ]; then bash scripts/local-dev.sh infra-down "$(DEV_STACK)"; else bash scripts/local-dev.sh down; fi

status:  ## List shared development stacks, registered worktrees and container states
	@bash scripts/local-dev.sh status

_ensure-local-daemon:
	@FOLLOW_LOGS=0 $(MAKE) --no-print-directory up

# One target for both the first fill and the monthly refresh — an empty catalogue
# emits INSERTs, a populated one emits UPSERTs for drift only, so the refresh path
# is exercised from day one instead of rotting as a rarely-run branch.
#
# Emit-and-review by default: rates are billing data, so nothing reaches the
# database until the diff has been read and APPLY=1 passed. Reads DATABASE_URL to
# diff against the live catalogue; unset means fresh-install mode.
seed-models:  ## Review or replace model identities and price rows (APPLY=1 to write)
	@bun scripts/seed-models.mjs $(if $(filter 1,$(APPLY)),--apply,)
