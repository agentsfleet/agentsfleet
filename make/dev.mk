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
	@if [ -n "$${LOCAL_DEV_BINARY:-}" ]; then bash scripts/local-dev.sh ensure; else FOLLOW_LOGS=0 $(MAKE) --no-print-directory up; fi

# One target for first fill and refresh, so the refresh path is always exercised.
# Rates are billing data: it prints a diff until APPLY=1. DATABASE_URL unset
# means fresh-install mode.
seed-models:  ## Review or replace model identities and price rows (APPLY=1 to write)
	@bun scripts/seed-models.mjs $(if $(filter 1,$(APPLY)),--apply,)
