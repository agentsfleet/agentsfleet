# =============================================================================
# DEV — local development
# =============================================================================

# The URL is derived without starting infrastructure or reading credentials.
LOCAL_DAEMON_URL := $(shell bash scripts/local-dev.sh url)

.PHONY: up down _ensure-local-daemon seed-models

up:  ## Reuse shared infrastructure and start this worktree's native daemon
	@bash scripts/local-dev.sh up
	@if [ "$${FOLLOW_LOGS:-1}" = "1" ]; then bash scripts/local-dev.sh logs; fi

down:  ## Stop only this worktree's daemon; preserve infrastructure and data
	@bash scripts/local-dev.sh down

_ensure-local-daemon:
	@FOLLOW_LOGS=0 $(MAKE) --no-print-directory up

# One target for both the first fill and the monthly refresh — an empty catalogue
# emits INSERTs, a populated one emits UPSERTs for drift only, so the refresh path
# is exercised from day one instead of rotting as a rarely-run branch.
#
# Emit-and-review by default: rates are billing data, so nothing reaches the
# database until the diff has been read and APPLY=1 passed. Reads DATABASE_URL to
# diff against the live catalogue; unset means fresh-install mode.
seed-models:  ## Seed/refresh core.model_library from the curated allowlist (APPLY=1 to write)
	@node scripts/seed-models.mjs $(if $(APPLY),--apply,)
