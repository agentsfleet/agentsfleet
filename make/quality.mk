# =============================================================================
# QUALITY — code quality, formatting, analysis
# =============================================================================

.PHONY: lint lint-zig lint-website lint-apps lint-ci openapi doctor check-pg-drain check-schema-gate _fmt _fmt_check _zlint_check _pg_drain_check _schema_gate_check _zig_target_lint _zig_line_limit_check _hardcoded_role_check _legacy_symbols_check _website_lint _app_lint _design_system_lint _zombiectl_lint _actionlint_check

ZLINT ?= zlint
ACTIONLINT ?= actionlint

_fmt:
	@echo "→ [zombied] Formatting Zig code..."
	@find src -name '*.zig' -exec zig fmt {} \;

_fmt_check:
	@echo "→ [zombied] Checking Zig formatting..."
	@find src -name '*.zig' -exec zig fmt --check {} \;

_zlint_check:
	@echo "→ [zombied] Running ZLint..."
	@command -v $(ZLINT) >/dev/null 2>&1 || { echo "ZLint not found. Install v0.7.9 or set ZLINT=/path/to/zlint."; exit 1; }
	@$(ZLINT) --deny-warnings
	@echo "✓ [zombied] ZLint passed"

_website_lint:
	@echo "→ [website] Running ESLint + TypeScript check..."
	@cd ui/packages/website && bun run lint
	@cd ui/packages/website && bun run typecheck
	@echo "✓ [website] Lint passed"

_app_lint:
	@echo "→ [app] Running ESLint + TypeScript check..."
	@cd ui/packages/app && bun run lint
	@cd ui/packages/app && bun run typecheck
	@echo "✓ [app] Lint passed"

_design_system_lint:
	@echo "→ [design-system] Running ESLint + TypeScript check..."
	@cd ui/packages/design-system && bun run lint
	@echo "✓ [design-system] Lint passed"

_zombiectl_lint:
	@echo "→ [zombiectl] Checking CLI syntax..."
	@cd zombiectl && node --check src/cli.js && node --check bin/zombiectl.js
	@echo "✓ [zombiectl] Lint passed"

_pg_drain_check:
	@echo "→ [zombied] Checking pg query drain discipline..."
	@python3 lint-zig.py src
	@echo "✓ [zombied] pg-drain check passed"

_zig_target_lint:
	@echo "→ [ci] Checking Zig target triples for -gnu suffix..."
	@FAIL=0; \
	for f in .github/workflows/*.yml; do \
		[ -f "$$f" ] || continue; \
		if grep -nE -- '-Dtarget=\S+-gnu\b' "$$f" >/dev/null 2>&1; then \
			echo "✗ $$f: found -gnu suffix (causes GLIBC mismatch):"; \
			grep -nE -- '-Dtarget=\S+-gnu\b' "$$f" | sed 's/^/    /'; \
			FAIL=1; \
		fi; \
	done; \
	if [ "$$FAIL" = "1" ]; then \
		echo "  Fix: use -Dtarget=x86_64-linux (not x86_64-linux-gnu)."; \
		echo "  Why: explicit -gnu makes Zig target GLIBC 2.17; system libssl needs 2.34+."; \
		exit 1; \
	fi; \
	echo "✓ [ci] No -gnu suffixes in Zig target triples"

# Files that already exceed 350 lines before this gate was tightened.
# Do NOT add new entries — shrink this list over time.
# Policy: RULE FLL in docs/greptile-learnings/RULES.md
ZIG_LINE_LIMIT_ALLOWLIST := \
	src/auth/claims.zig \
	src/auth/github.zig \
	src/auth/jwks.zig \
	src/cmd/doctor.zig \
	src/cmd/serve.zig \
	src/config/runtime.zig \
	src/db/pool.zig \
	src/executor/client.zig \
	src/executor/handler.zig \
	src/executor/runner.zig \
	src/executor/session.zig \
	src/executor/transport.zig \
	src/git/pr.zig \
	src/git/repo.zig \
	src/http/handlers/common.zig \
	src/http/workspace_guards.zig \
	src/observability/metrics_counters.zig \
	src/observability/otel_logs.zig \
	src/observability/otel_traces.zig \
	src/observability/posthog_events.zig \
	src/queue/redis_client.zig \
	src/state/entitlements.zig \
	src/state/topology.zig \
	src/types.zig \
	src/types/id_format.zig \
	src/zombie/approval_gate.zig \
	src/zombie/config.zig \
	src/zombie/event_loop.zig

_zig_line_limit_check:
	@echo "→ [zombied] Checking Zig file line limit (max 350 lines — RULE FLL)..."
	@FAIL=0; \
	for f in $$(find src -name '*.zig' ! -path '*/.zig-cache/*' ! -name '*_test.zig' ! -name '*_test_*.zig' ! -name 'tests*.zig' ! -name '*test*.zig' | sort); do \
		lines=$$(wc -l < "$$f"); \
		if [ "$$lines" -gt 350 ]; then \
			allowed=0; \
			for a in $(ZIG_LINE_LIMIT_ALLOWLIST); do \
				[ "$$f" = "$$a" ] && allowed=1 && break; \
			done; \
			if [ "$$allowed" = "0" ]; then \
				echo "✗ $$f: $$lines lines (limit 350 — RULE FLL)"; \
				FAIL=1; \
			fi; \
		fi; \
	done; \
	if [ "$$FAIL" = "1" ]; then \
		echo "  Fix: split the file into focused modules under 350 lines."; \
		exit 1; \
	fi; \
	echo "✓ [zombied] All new Zig files within 350-line limit"

_hardcoded_role_check:
	@echo "→ [zombied] Checking for banned hardcoded role constants..."
	@FAIL=0; \
	if grep -rn 'ROLE_SCOUT\|ROLE_ECHO\|ROLE_WARDEN' src/ --include='*.zig' | grep -v '_test\.zig' | grep -q .; then \
		echo "✗ Banned role constants found (ROLE_SCOUT/ROLE_ECHO/ROLE_WARDEN). Remove them — roles are loaded from config."; \
		grep -rn 'ROLE_SCOUT\|ROLE_ECHO\|ROLE_WARDEN' src/ --include='*.zig' | grep -v '_test\.zig'; \
		FAIL=1; \
	fi; \
	if grep -rn 'eqlIgnoreCase.*"echo"\|eqlIgnoreCase.*"scout"\|eqlIgnoreCase.*"warden"' src/ --include='*.zig' | grep -v '_test\.zig' | grep -q .; then \
		echo "✗ Hardcoded role string comparison found. Use the active profile skill list instead."; \
		grep -rn 'eqlIgnoreCase.*"echo"\|eqlIgnoreCase.*"scout"\|eqlIgnoreCase.*"warden"' src/ --include='*.zig' | grep -v '_test\.zig'; \
		FAIL=1; \
	fi; \
	if grep -rn 'mem\.eql.*"echo"\|mem\.eql.*"scout"\|mem\.eql.*"warden"' src/ --include='*.zig' | grep -v '_test\.zig' | grep -q .; then \
		echo "✗ Hardcoded role string comparison (mem.eql) found. Use the active profile skill list instead."; \
		grep -rn 'mem\.eql.*"echo"\|mem\.eql.*"scout"\|mem\.eql.*"warden"' src/ --include='*.zig' | grep -v '_test\.zig'; \
		FAIL=1; \
	fi; \
	if [ "$$FAIL" = "1" ]; then exit 1; fi; \
	echo "✓ [zombied] No hardcoded role constants found"

_actionlint_check:
	@echo "→ [ci] Running actionlint on GitHub Actions workflows..."
	@command -v $(ACTIONLINT) >/dev/null 2>&1 || { echo "actionlint not found. Install via: mise install actionlint"; exit 1; }
	@$(ACTIONLINT) .github/workflows/*.yml
	@echo "✓ [ci] actionlint passed"

check-pg-drain: _pg_drain_check  ## Check that all conn.query() calls have a .drain()

_schema_gate_check:
	@echo "→ [zombied] Checking schema/*.sql against pre-v2.0 teardown convention..."
	@version=$$(cat VERSION); \
	major=$$(echo "$$version" | cut -d. -f1); \
	if [ "$$major" -ge 2 ]; then \
		echo "  (VERSION=$$version ≥ 2.0.0 — teardown convention relaxed, skipping)"; \
		exit 0; \
	fi; \
	FAIL=0; \
	for f in schema/*.sql; do \
		[ -f "$$f" ] || continue; \
		if grep -nE '^\s*(ALTER\s+TABLE|DROP\s+TABLE|DROP\s+COLUMN)\b' "$$f" >/dev/null 2>&1; then \
			echo "✗ $$f: ALTER/DROP forbidden pre-v2.0 (VERSION=$$version)"; \
			grep -nE '^\s*(ALTER\s+TABLE|DROP\s+TABLE|DROP\s+COLUMN)\b' "$$f" | sed 's/^/    /'; \
			FAIL=1; \
		fi; \
		if grep -nE '^\s*SELECT\s+1\s*;\s*(--|$$)' "$$f" >/dev/null 2>&1; then \
			echo "✗ $$f: 'SELECT 1;' version marker is a comment-only migration shim (forbidden pre-v2.0)"; \
			FAIL=1; \
		fi; \
	done; \
	if [ "$$FAIL" = "1" ]; then \
		echo "  Fix: pre-v2.0 removes tables by deleting the slot file + embed.zig + migration array entry."; \
		echo "  See CLAUDE.md → 'Schema Table Removal Guard'."; \
		exit 1; \
	fi; \
	echo "✓ [zombied] schema-gate check passed (VERSION=$$version, pre-v2.0 teardown convention)"

check-schema-gate: _schema_gate_check  ## Enforce pre-v2.0 teardown convention on schema/*.sql

REDOCLY := bun x redocly

openapi:  ## Bundle YAML → openapi.json, lint, check error schema, self-test
	@echo "→ [openapi] Bundling split YAML → public/openapi.json..."
	@$(REDOCLY) bundle public/openapi/root.yaml -o public/openapi.json >/dev/null
	@echo "→ [openapi] Redocly lint..."
	@$(REDOCLY) lint public/openapi.json --config .redocly.yaml
	@echo "→ [openapi] ErrorBody + application/problem+json schema check..."
	@python3 scripts/check_openapi_errors.py
	@echo "→ [openapi] REST §1 URL shape (no verbs in URLs)..."
	@python3 scripts/check_openapi_url_shape.py
	@echo "✓ [openapi] Bundle + lint + error-schema + url-shape all green"

_legacy_symbols_check:
	@echo "→ [zombied] Checking for legacy event-substrate symbols (orphan sweep — RULE ORP)..."
	@FAIL=0; \
	PATTERNS='\bactivity_events\b|\bactivity_stream\b|\bactivity_cursor\b|\bzombie_steer_key_suffix\b|"GETDEL".*"zombie:'; \
	HITS=$$(grep -rEn "$$PATTERNS" src/ --include='*.zig' \
	         | grep -vE '^[^:]+:[0-9]+:[ \t]*//' || true); \
	if [ -n "$$HITS" ]; then \
		echo "✗ Legacy event-substrate symbols found in active code (RULE ORP). Strip or replace — these were removed in slice 1/8 of the unified event substrate:"; \
		echo "$$HITS"; \
		FAIL=1; \
	fi; \
	if [ $$FAIL -eq 1 ]; then exit 1; fi; \
	echo "✓ [zombied] No legacy event-substrate symbols in active code"

lint-zig: _fmt_check _zlint_check _pg_drain_check _schema_gate_check _zig_target_lint _zig_line_limit_check _hardcoded_role_check _legacy_symbols_check  ## Lint zombied (Zig)
	@echo "✓ [zombied] Lint passed"

lint-website: _website_lint  ## Lint website only (ESLint + tsc)

lint-apps: _app_lint _design_system_lint _zombiectl_lint  ## Lint app, design-system, and zombiectl

lint-ci: _actionlint_check  ## Lint GitHub Actions workflows (actionlint)

lint: lint-zig lint-website lint-apps lint-ci openapi  ## Lint everything (zombied + website + app + design-system + zombiectl + CI + OpenAPI)
	@echo "✓ All lint checks passed"

doctor:  ## Run zombied doctor (connectivity + config check)
	@zig build run -- doctor
