# =============================================================================
# QUALITY — code quality, formatting, analysis
# =============================================================================

.PHONY: lint-all lint-zig lint-governance lint-website lint-apps-ds-ctl lint-app lint-design-system lint-cli lint-shell check-documentation-rules check-openapi check-gh-actions-valid check-playbooks check-route-registration-doc gen-error-codes _fmt _fmt_check _zlint_check _lint_zig_pg_drain _lint_zig_discipline _lint_zig_test_depth _zig_target_lint _zig_line_limit_check _hardcoded_role_check _legacy_symbols_check _website_lint _app_lint _design_system_lint _cli_lint _shell_lint

# Regenerate docs/api-reference/error-codes.mdx (own repo, ~/Projects/docs)
# from the agentsfleetd error registry. No default target path on purpose —
# cross-repo writes to ~/Projects/docs/ need an explicit per-session path
# (own-branch workflow), never a silent default.
gen-error-codes:  ## Regenerate error-codes.mdx from the error registry — usage: make gen-error-codes ERROR_CODES_MDX=/path/to/error-codes.mdx
	@test -n "$(ERROR_CODES_MDX)" || { echo "usage: make gen-error-codes ERROR_CODES_MDX=/path/to/error-codes.mdx"; exit 1; }
	@echo "→ [errors] generating $(ERROR_CODES_MDX) from the registry..."
	@zig build gen-error-codes > "$(ERROR_CODES_MDX).tmp" && mv "$(ERROR_CODES_MDX).tmp" "$(ERROR_CODES_MDX)"
	@echo "✓ [errors] $(ERROR_CODES_MDX) regenerated"

check-documentation-rules:  ## Check public API and command help text
	@PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_documentation_rules_test.py
	@PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_documentation_rules.py

ZLINT ?= zlint
ACTIONLINT ?= actionlint

_fmt:
	@echo "→ [zig] Formatting Zig code..."
	@find src -name '*.zig' -exec zig fmt {} \;

_fmt_check:
	@echo "→ [zig] Checking Zig formatting..."
	@find src -name '*.zig' -exec zig fmt --check {} \;

_zlint_check:
	@echo "→ [zig] Running ZLint..."
	@command -v $(ZLINT) >/dev/null 2>&1 || { echo "ZLint not found. Install v0.8.1 or set ZLINT=/path/to/zlint."; exit 1; }
	@$(ZLINT) --deny-warnings
	@echo "✓ [zig] ZLint passed"

_website_lint:
	@echo "→ [website] Running Oxlint + TypeScript check..."
	@cd ui/packages/website && bun run lint
	@cd ui/packages/website && bun run typecheck
	@echo "✓ [website] Lint passed"

_app_lint:
	@echo "→ [app] Running Oxlint + TypeScript check..."
	@cd ui/packages/app && bun run lint
	@cd ui/packages/app && bun run typecheck
	@echo "✓ [app] Lint passed"

_design_system_lint:
	@echo "→ [design-system] Running Oxlint + TypeScript check..."
	@cd ui/packages/design-system && bun run lint
	@echo "✓ [design-system] Lint passed"

_cli_lint:
	@echo "→ [agentsfleet] Oxlint + runtime/const audits + tsc..."
	@cd cli && bun run lint
	@echo "✓ [agentsfleet] Lint passed"

_lint_zig_pg_drain:
	@echo "→ [zig] Checking pg query drain discipline..."
	@python3 lint-zig.py src
	@echo "✓ [zig] pg-drain check passed"

# Roster-scoped ghostty-derived discipline (A5 poison + ownership phrase blocking
# inside audits/zig-discipline-roster.txt; A2 errdefer heuristic advisory), plus
# the fixture-driven self-tests that prove each check bites in/out of the roster.
DISCIPLINE_TESTS := python3 -m unittest discover -s scripts -t scripts -p 'check_zig_discipline*_test.py'

_lint_zig_discipline:
	@echo "→ [zig] Checking ghostty-derived A5/A2 discipline (roster-scoped)..."
	@python3 lint-zig.py --discipline --roster audits/zig-discipline-roster.txt src
	@echo "→ [zig] Discipline lint self-tests..."
	@$(DISCIPLINE_TESTS)
	@echo "✓ [zig] discipline check + self-tests passed"

# Governance gates: the script-driven checks that enforce repository CONVENTIONS
# rather than compile correctness. Grouped under one target so `lint-zig` names a
# policy set instead of a growing list, and so a new rule extends this line
# rather than adding another near-duplicate wrapper.
#
# Deliberately NOT folded in: _fmt_check / _zlint_check (tooling, not policy) and
# check-test-reachability / _lint_zig_test_depth (test structure, and the latter
# is invoked directly to record a spec's test baseline).
lint-governance: _lint_zig_pg_drain _lint_zig_discipline _zig_line_limit_check _hardcoded_role_check _legacy_symbols_check _legacy_noun_check _runner_isolation_check  ## Run the repository convention gates
	@echo "✓ [governance] All convention gates passed"

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
	src/config/runtime.zig \
	src/db/pool.zig \
	src/git/pr.zig \
	src/git/repo.zig \
	src/http/workspace_guards.zig \
	src/observability/metrics_counters.zig \
	src/observability/posthog_events.zig \
	src/queue/redis_client.zig \
	src/state/entitlements.zig \
	src/state/topology.zig \
	src/types.zig \
	src/types/id_format.zig \
	src/agent/approval_gate.zig \
	src/agent/config.zig

ZIG_LINE_LIMIT_EXCLUDE_DIRS := (^|/)(vendor|third_party|\.zig-cache)/
ZIG_LINE_LIMIT_TEST_PATTERN := (^|/)(tests?)/|_test\.zig$$|_test_.*\.zig$$|tests\.zig$$|.*test.*\.zig$$

# `-c safe.directory=*` because the CI container runs as root against a checkout
# owned by the runner user; plain `git ls-files` there dies with "dubious ownership"
# (exit 128). The empty-list check is the real guard: without it the loop below
# never runs, FAIL stays 0, and this gate prints ✓ having inspected zero files —
# which is exactly what it did in CI until Jul 2026.
_zig_line_limit_check:
	@echo "→ [zig] Checking Zig file line limit (max 350 lines — RULE FLL)..."
	@FAIL=0; \
	files=$$(git -c safe.directory='*' ls-files '*.zig' | grep -vE '$(ZIG_LINE_LIMIT_EXCLUDE_DIRS)' | grep -vE '$(ZIG_LINE_LIMIT_TEST_PATTERN)' | sort); \
	if [ -z "$$files" ]; then \
		echo "✗ [zig] line-limit gate listed zero Zig files — git failed, so this gate proved nothing"; \
		exit 1; \
	fi; \
	for f in $$files; do \
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
	echo "✓ [zig] All new Zig files within 350-line limit"

_hardcoded_role_check:
	@echo "→ [zig] Checking for banned hardcoded role constants..."
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
	echo "✓ [zig] No hardcoded role constants found"



REDOCLY := bunx @redocly/cli

ROUTE_COVERAGE_TESTS := python3 -m unittest discover -s scripts -t scripts -p 'check_openapi_route_coverage*_test.py'

check-openapi: check-documentation-rules  ## Bundle YAML → openapi.json + public-text + schema + route checks
	@echo "→ [openapi] Bundling split YAML → public/openapi.json..."
	@$(REDOCLY) bundle public/openapi/root.yaml -o public/openapi.json >/dev/null
	@echo "→ [openapi] Redocly lint..."
	@$(REDOCLY) lint public/openapi.json --config .redocly.yaml
	@echo "→ [openapi] ErrorBody + application/problem+json schema check..."
	@python3 scripts/check_openapi_errors.py
	@echo "→ [openapi] REST §1 URL shape (no verbs in URLs)..."
	@python3 scripts/check_openapi_url_shape.py
	@echo "→ [openapi] Route-coverage gate self-tests..."
	@$(ROUTE_COVERAGE_TESTS)
	@echo "→ [openapi] REST §7 served-vs-documented route coverage..."
	@python3 scripts/check_openapi_route_coverage.py
	@echo "✓ [openapi] Bundle + lint + error-schema + url-shape + route-coverage all green"

check-route-registration-doc:  ## REST guide §7 route-registration facts stay fresh (middleware names, cited paths, make targets, dead prefixes)
	@python3 scripts/check_route_registration_doc_test.py
	@python3 scripts/check_route_registration_doc.py

SHELLCHECK ?= shellcheck

_shell_lint:
	@echo "→ [shell] Running shellcheck on scripts/*.sh..."
	@command -v $(SHELLCHECK) >/dev/null 2>&1 || { echo "shellcheck not found. Install via: mise install shellcheck"; exit 1; }
	@# `--severity=error` is the floor: catches genuine breakage (syntax,
	@# undefined-vars, dangerous quoting) without blocking on pre-existing
	@# stylistic warnings in symlinked dotfiles/scripts/. Tighten to
	@# `warning` once dotfiles cleanup lands.
	@# `-x` lets shellcheck follow `source`/`.` into sibling scripts.
	@$(SHELLCHECK) --severity=error -x scripts/*.sh
	@echo "✓ [shell] shellcheck passed (error-level)"

_legacy_symbols_check:
	@echo "→ [zig] Checking for legacy event-substrate symbols (orphan sweep — RULE ORP)..."
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
	echo "✓ [zig] No legacy event-substrate symbols in active code"

_legacy_noun_check:
	@echo "→ [noun] Checking for the retired entity noun (zombie_id/zmb_id) in src/ + schema/ — the product noun is 'fleet'..."
	@FAIL=0; \
	NOUN_PATTERNS='\bzombie_id\b|\bzmb_id\b'; \
	HITS=$$(grep -rEn "$$NOUN_PATTERNS" src/ schema/ --include='*.zig' --include='*.sql' \
	         | grep -vE '^[^:]+:[0-9]+:[ \t]*(//|--)' || true); \
	if [ -n "$$HITS" ]; then \
		echo "✗ Retired entity identifier (zombie_id/zmb_id) found in active code — the product noun is 'fleet'; use fleet_id:"; \
		echo "$$HITS"; \
		FAIL=1; \
	fi; \
	if [ $$FAIL -eq 1 ]; then exit 1; fi; \
	echo "✓ [noun] No retired zombie_id/zmb_id identifiers in src/ + schema/"

_runner_isolation_check:
	@echo "→ [isolation] Verifying the runner graph (build_runner.zig + src/build/shared.zig) depends ONLY on nullclaw — zero datastore/server deps (pg/s3/httpz)..."
	@FAIL=0; \
	DEP_HITS=$$(grep -En 'b\.dependency\(' build_runner.zig src/build/shared.zig \
	         | grep -vE '^[^:]+:[0-9]+:[ \t]*//' \
	         | grep -vE 'S_NULLCLAW|"nullclaw"' || true); \
	HELPER_HITS=$$(grep -En 'buildpkg\.(pg|s3)\b' build_runner.zig src/build/shared.zig \
	         | grep -vE '^[^:]+:[0-9]+:[ \t]*//' || true); \
	IMPORT_HITS=$$(grep -En '@import\("([^"]*/)?(pg|s3)\.zig"\)' build_runner.zig src/build/shared.zig \
	         | grep -vE '^[^:]+:[0-9]+:[ \t]*//' || true); \
	if [ -n "$$DEP_HITS$$HELPER_HITS$$IMPORT_HITS" ]; then \
		echo "✗ Runner isolation breach — the runner graph may depend ONLY on nullclaw (no pg/s3/httpz; no direct @import of the daemon-only helpers). Offending lines:"; \
		[ -n "$$DEP_HITS" ] && echo "$$DEP_HITS"; \
		[ -n "$$HELPER_HITS" ] && echo "$$HELPER_HITS"; \
		[ -n "$$IMPORT_HITS" ] && echo "$$IMPORT_HITS"; \
		FAIL=1; \
	fi; \
	if [ $$FAIL -eq 1 ]; then exit 1; fi; \
	echo "✓ [isolation] runner graph depends only on nullclaw (no pg/s3/httpz)"

lint-zig: _fmt_check _zlint_check lint-governance check-test-reachability _lint_zig_test_depth _zig_target_lint  ## Lint all Zig source (agentsfleetd/runner/lib)
	@echo "✓ [zig] Lint passed"

lint-website: _website_lint  ## Lint website only (Oxlint + tsc)

lint-apps-ds-ctl: _app_lint _design_system_lint _cli_lint  ## Lint app + design-system + agentsfleet

lint-app: _app_lint  ## Lint ui/packages/app only (Oxlint + tsc)

lint-design-system: _design_system_lint  ## Lint ui/packages/design-system only (Oxlint + tsc)

lint-cli: _cli_lint check-documentation-rules  ## Lint agentsfleet CLI and its public text

lint-shell: _shell_lint  ## Lint scripts/*.sh via shellcheck (follows dotfiles symlinks)

lint-all: lint-zig lint-website lint-apps-ds-ctl lint-shell check-documentation-rules check-openapi check-gh-actions-valid check-playbooks check-route-registration-doc check-architecture-doc check-deploy-safety  ## Run all linters + quality gates
	@echo "✓ All lint checks passed"

check-gh-actions-valid:  ## Validate .github/workflows/ — actionlint (YAML + run: shellcheck) + make-target ref check
	@echo "→ [gh-actions] Running actionlint on workflows..."
	@command -v $(ACTIONLINT) >/dev/null 2>&1 || { echo "actionlint not found. Install via: mise install actionlint"; exit 1; }
	@$(ACTIONLINT) .github/workflows/*.yml
	@echo "→ [gh-actions] Verifying make targets referenced in workflows..."
	@# Filter out our own recipe name — GNU make recurses on $(MAKE) even in
	@# -n mode (dry-run propagates through sub-makes), so a self-reference
	@# fork-bombs: each generation forks N sub-makes that each fork N more.
	@#
	@# Regex covers both `run: make <tgt>` (single-line) and `^<indent>make <tgt>`
	@# (continuation inside `run: |` blocks). Without the second pattern, multi-
	@# line shell blocks slip through (e.g. lint.yml's openapi assertion).
	@#
	@# Existence check greps stderr for "No rule to make target" rather than
	@# trusting `$(MAKE) -n`'s exit code. Recipes containing $(MAKE) execute
	@# even in dry-run (GNU make's recursion-propagation rule), so a target
	@# whose recipe touches the environment (e.g. valgrind probe) can exit
	@# non-zero in CI without being "unknown" — that's a false positive for
	@# the existence check we want here.
	@FAIL=0; \
	TGTS=$$( \
	  { grep -hoE 'run:[[:space:]]*make[[:space:]]+[A-Za-z0-9_./-]+' .github/workflows/*.yml .github/workflows/*.yaml 2>/dev/null; \
	    grep -hoE '^[[:space:]]+make[[:space:]]+[A-Za-z0-9_./-]+' .github/workflows/*.yml .github/workflows/*.yaml 2>/dev/null; \
	  } | awk '{print $$NF}' | grep -v '^check-gh-actions-valid$$' | sort -u); \
	for tgt in $$TGTS; do \
	  err=$$($(MAKE) -n "$$tgt" 2>&1 >/dev/null || true); \
	  if echo "$$err" | grep -qE "No rule to make target [\`']?$$tgt[\`']?"; then \
	    echo "✗ '.github/workflows/' references 'make $$tgt' which is not a known target"; \
	    FAIL=1; \
	  fi; \
	done; \
	if [ $$FAIL -eq 1 ]; then echo "✗ workflow target reference check failed"; exit 1; fi; \
	echo "✓ [gh-actions] actionlint + make-target refs all green"

check-playbooks: check-vault-gate-parity  ## Validate playbooks/ — vault-gate parity + shellcheck + reference integrity + README/tree parity
	@echo "→ [playbooks] shellcheck on playbooks/**/*.sh..."
	@command -v $(SHELLCHECK) >/dev/null 2>&1 || { echo "shellcheck not found. Install via: mise install shellcheck"; exit 1; }
	@find playbooks -name '*.sh' -print0 | xargs -0 $(SHELLCHECK) --severity=error -x
	@echo "→ [playbooks] focused shell regression tests..."
	@set -e; for test_script in playbooks/founding/02_preflight/credentials_test.sh playbooks/founding/06_runner_bootstrap_dev/provision_runner_env_test.sh; do bash "$$test_script"; done
	@echo "→ [playbooks] reference integrity — every playbooks/ path resolves..."
	@# Scans the live operational surface (CI, scripts, active docs, the playbooks
	@# themselves). Excludes docs/v2/: specs are historical records that
	@# intentionally cite now-moved paths.
	@FAIL=0; \
	REFS=$$(git -c safe.directory='*' grep -hoE 'playbooks/[A-Za-z0-9_./-]+' -- . ':!docs/v2/' | sed 's/[.,):]*$$//' | sort -u); \
	if [ -z "$$REFS" ]; then echo "✗ [playbooks] reference scan matched nothing — git failed, so this gate proved nothing"; exit 1; fi; \
	for ref in $$REFS; do \
	  [ -e "$$ref" ] || { echo "✗ broken playbooks/ reference: $$ref"; FAIL=1; }; \
	done; \
	if [ $$FAIL -eq 1 ]; then echo "✗ [playbooks] reference integrity failed"; exit 1; fi; \
	echo "✓ [playbooks] all references resolve"
	@echo "→ [playbooks] README ↔ tree parity..."
	@FAIL=0; seen=""; \
	for d in $$(find playbooks/founding playbooks/operations -type d); do \
	  [ -f "$$d/001_playbook.md" ] || continue; \
	  base=$$(basename "$$d"); \
	  case " $$seen " in *" $$base "*) echo "✗ duplicate playbook basename '$$base' — README parity is basename-matched (tree shows leaf names) and cannot disambiguate: $$d"; FAIL=1 ;; *) seen="$$seen $$base" ;; esac; \
	  grep -q "$$base/" playbooks/README.md || { echo "✗ playbook dir absent from README tree: $$d"; FAIL=1; }; \
	done; \
	if [ $$FAIL -eq 1 ]; then echo "✗ [playbooks] README/tree parity failed"; exit 1; fi; \
	echo "✓ [playbooks] README documents every playbook dir"
