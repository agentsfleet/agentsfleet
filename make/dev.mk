# =============================================================================
# DEV — local development
# =============================================================================

.PHONY: dev up down _clean env

VERSION ?= $(shell cat VERSION 2>/dev/null || echo "0.1.0")

up:  ## Start all services and tail app logs
	@echo "Starting UseZombie..."
	@docker compose up -d
	@echo ""
	@echo "Services:"
	@echo "  API:       http://localhost:3000"
	@echo "  Postgres:  localhost:5432"
	@echo ""
	@if [ "$${FOLLOW_LOGS:-1}" = "1" ]; then \
		docker compose logs -f zombied; \
	fi

dev: up  ## Alias for 'make up'

down:  ## Stop all services, remove volumes, and cleanup
	@echo "Stopping all services..."
	@docker compose down --volumes
	@$(MAKE) _clean --no-print-directory
	@echo "Cleanup complete."

env:  ## Generate .env from Proton Pass vault (ENV=local|dev|prod)
	@pass-cli inject -i .env.$(or $(ENV),local).tpl -o .env -f
	@chmod 600 .env
	@echo "✔ Generated .env from .env.$(or $(ENV),local).tpl"

_clean:
	@rm -rf zig-out zig-cache .zig-cache
