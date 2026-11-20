.PHONY: check-models lint-all lint-source lint-rustd lint-website lint-app lint-design-system lint-cli lint-shell check-gh-actions-valid check-playbooks check-playbooks-refs

ACTIONLINT ?= actionlint

# ast-grep source rules (sgconfig.yml). `test` first: each rule must refuse its
# regression before a clean scan means anything. `--no-install` runs only the
# pinned version. `--no-ignore hidden` reaches .github/; .git/ is excluded by glob.
AST_GREP := bunx --no-install ast-grep

lint-source:  ## Source rules over code, docs, tests, workflows and infra (ast-grep test + scan)
	@echo "→ [source] ast-grep rule tests, then the scan..."
	@$(AST_GREP) test --skip-snapshot-tests
	@$(AST_GREP) scan --no-ignore hidden --globs '!.git/**'
	@echo "✓ [source] clean"

lint-website:  ## Lint website only (Oxlint + tsrs)
	@echo "→ [website] Running Oxlint + TypeScript check..."
	@cd ui/packages/website && bun run lint
	@cd ui/packages/website && bun run typecheck
	@echo "✓ [website] Lint passed"

lint-app: lint-source  ## Lint ui/packages/app only (Oxlint + tsrs + source rules)
	@echo "→ [app] Running Oxlint + TypeScript check..."
	@cd ui/packages/app && bun run lint
	@cd ui/packages/app && bun run typecheck
	@echo "✓ [app] Lint passed"

lint-design-system: lint-source  ## Lint ui/packages/design-system only (Oxlint + tsrs + source rules)
	@echo "→ [design-system] Running Oxlint + TypeScript check..."
	@cd ui/packages/design-system && bun run lint
	@echo "✓ [design-system] Lint passed"

lint-cli: lint-source  ## Lint agentsfleet CLI and its public text
	@echo "→ [agentsfleet] Oxlint + runtime/const audits + tsrs..."
	@cd cli && bun run lint
	@echo "✓ [agentsfleet] Lint passed"

# A repository convention rather than compile correctness, so it rides lint-all.
check-models:  ## Check the curated model catalogue: identities, context evidence and exact sourced prices
	@echo "→ [models] Checking curated identities, context evidence and exact sourced prices..."
	@bun run --bun tsrs --project scripts/model-catalogue/tsconfig.json
	@bun test scripts/model-catalogue
	@bun scripts/model-catalogue/check.mjs

# `cd` rather than `--manifest-path`: rustup reads rust-toolchain.toml from the
# working directory. No `lint-source` prerequisite: lint-all already runs it once.
# `--all-targets`, so a test file is no refuge from clippy.
RUSTD_DIR := rustd
# The version CI installs too (`.github/workflows/test-unit-rustd.yml`).
CARGO_SHEAR_VERSION := 1.14.0

lint-rustd:  ## Lint the Rust workspace (rustfmt + clippy + cargo shear, warnings are errors)
	@command -v cargo >/dev/null 2>&1 || { echo "✗ cargo not found. Install via: mise install rust"; exit 1; }
	@command -v cargo-shear >/dev/null 2>&1 || { echo "✗ cargo-shear not found. Install via: mise use -g cargo:cargo-shear@$(CARGO_SHEAR_VERSION)"; exit 1; }
	@PYTHONDONTWRITEBYTECODE=1 python3 scripts/rustd_coverage_test.py
	@cd $(RUSTD_DIR) && $(WITH_PROGRESS) "[rustd] rustfmt --check" -- cargo fmt --check
	@# --all-features: `test-util` code compiles only with it, and lint must see it.
	@cd $(RUSTD_DIR) && $(WITH_PROGRESS) "[rustd] clippy -D warnings" -- \
	  cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
	@# Dead-code lints stop at a crate's edge; shear fails unused dependencies.
	@# Its unlinked-file notes stay warnings: `#[path]` suites read as unlinked.
	@cd $(RUSTD_DIR) && $(WITH_PROGRESS) "[rustd] cargo shear" -- cargo shear --locked

SHELLCHECK ?= shellcheck

lint-shell:  ## shellcheck scripts/*.sh at error level, and the agentsfleet.dev installer and its test at every level
	@echo "→ [shell] Running shellcheck on scripts/*.sh and the installer..."
	@command -v $(SHELLCHECK) >/dev/null 2>&1 || { echo "shellcheck not found. Install via: mise install shellcheck"; exit 1; }
	@# Error level is the floor for scripts/ until its warnings are fixed;
	@# `-x` follows `source` into sibling scripts.
	@$(SHELLCHECK) --severity=error -x scripts/*.sh
	@# The installer is what `curl | bash` runs, so it meets every severity.
	@$(SHELLCHECK) -s bash ui/agentsfleet.dev/dist/install.sh ui/agentsfleet.dev/install_test.sh
	@echo "✓ [shell] shellcheck passed (scripts/ at error level, installer at every level)"

lint-all: lint-rustd lint-source check-models lint-website lint-app lint-design-system lint-cli lint-shell check-gh-actions-valid check-playbooks check-deploy-safety  ## Run all linters + quality gates
	@echo "✓ All lint checks passed"

check-gh-actions-valid:  ## Validate .github/ — actionlint (YAML + run: shellcheck), make-target refs in workflows and actions, check-release-candidate
	@echo "→ [gh-actions] Running actionlint on workflows..."
	@command -v $(ACTIONLINT) >/dev/null 2>&1 || { echo "actionlint not found. Install via: mise install actionlint"; exit 1; }
	@$(ACTIONLINT) .github/workflows/*.yml
	@# What workflows run that actionlint cannot see is ast-grep's (lint-source).
	@echo "→ [gh-actions] Verifying make targets referenced in workflows and actions..."
	@# A self-reference is filtered out: $(MAKE) recurses even under -n, so it
	@# would fork-bomb. `make -C`/`-f` names another tree and is skipped. Existence
	@# is read from "No rule to make target" on stderr, not the exit code, because
	@# a recipe with $(MAKE) executes under -n and may fail for other reasons.
	@FAIL=0; \
	TGTS=$$(awk '{ \
	    if (!match($$0, /^[[:space:]]*(-[[:space:]]+)?(run:[[:space:]]*)?make[[:space:]]/) && \
	        !match($$0, /(sudo|\$$\()[[:space:]]*make[[:space:]]/)) next; \
	    rest = substr($$0, RSTART + RLENGTH); \
	    sub(/[0-9]*[;|&)<>#].*/, "", rest); \
	    n = split(rest, word, /[[:space:]]+/); \
	    for (i = 1; i <= n; i++) { \
	      if (word[i] == "") continue; \
	      if (word[i] ~ /^(-C|-f|--directory|--file|--makefile)/) break; \
	      if (word[i] ~ /^-/) continue; \
	      if (word[i] !~ /^[A-Za-z0-9_][A-Za-z0-9_.\/-]*$$/) break; \
	      print word[i]; \
	    } \
	  }' $$(find .github/workflows -name '*.yml' -o -name '*.yaml') $$(find .github/actions -type f) \
	  | grep -v '^check-gh-actions-valid$$' | sort -u); \
	if [ -z "$$TGTS" ]; then echo "✗ [gh-actions] make-target scan matched nothing — the scan is broken, not the tree"; exit 1; fi; \
	for tgt in $$TGTS; do \
 err=$$($(MAKE) -n "$$tgt" 2>&1 >/dev/null || true); \
 if echo "$$err" | grep -qE "No rule to make target [\`']?$$tgt[\`']?"; then \
 echo "✗ '.github/' references 'make $$tgt' which is not a known target"; \
	    FAIL=1; \
 fi; \
	done; \
	if [ $$FAIL -eq 1 ]; then echo "✗ workflow target reference check failed"; exit 1; fi; \
	echo "✓ [gh-actions] actionlint + make-target refs all green"

# Deployment inputs dropped before each playbook suite, so a developer's shell
# cannot change a result (AGENTSFLEET_API_URL and VAULT_DEV both have). CI sets
# none of them.
PLAYBOOK_TEST_SCRUB = -u ENV -u STAGE -u ACTION -u PUSH -u REVISION \
  -u VAULT -u VAULT_DEV -u VAULT_PROD -u RUNNER_ITEM -u AGENTSFLEET_API_URL \
  -u RUNNER_STREAM_CA_CERTIFICATE -u RUNNER_STREAM_SERVER_NAME -u RUNNER_STREAM_WIREGUARD_CONFIG

# Split by cost: check-playbooks-refs is the cheap half a Makefile or workflow
# edit can break, and .githooks/pre-commit routes on that.
check-playbooks: check-playbooks-refs check-vault-gate-parity  ## Validate playbooks/ — vault-gate parity + shellcheck + regression tests + reference integrity
	@echo "→ [playbooks] shellcheck on playbooks/**/*.sh..."
	@command -v $(SHELLCHECK) >/dev/null 2>&1 || { echo "shellcheck not found. Install via: mise install shellcheck"; exit 1; }
	@find playbooks -name '*.sh' -print0 | xargs -0 $(SHELLCHECK) --severity=error -x
	@echo "→ [playbooks] focused shell regression tests (bounded parallel)..."
	@PLAYBOOK_TEST_SCRUB="$(PLAYBOOK_TEST_SCRUB)" bash scripts/run-playbook-tests.sh

check-playbooks-refs:  ## playbooks/ reference integrity only (cheap; what a Makefile edit can break)
	@echo "→ [playbooks] reference integrity — every playbooks/ path resolves..."
	@# docs/v2/ is excluded: specs cite moved paths on purpose.
	@FAIL=0; \
	REFS=$$(git grep -h -E 'playbooks/[A-Za-z0-9_./-]+' -- . ':(exclude)docs/v2/**' | \
	awk '{ text = $$0; while (match(text, /playbooks\/[A-Za-z0-9_.\/-]+/)) { ref = substr(text, RSTART, RLENGTH); sub(/[.,):]*$$/, "", ref); print ref; text = substr(text, RSTART + RLENGTH); } }' | sort -u); \
	if [ -z "$$REFS" ]; then echo "✗ [playbooks] reference scan matched nothing"; exit 1; fi; \
	for ref in $$REFS; do \
	  [ -e "$$ref" ] || { echo "✗ broken playbooks/ reference: $$ref"; FAIL=1; }; \
	done; \
	if [ $$FAIL -eq 1 ]; then echo "✗ [playbooks] reference integrity failed"; exit 1; fi; \
	echo "✓ [playbooks] all references resolve"

.PHONY: check-runner-collector-admission
check-deploy-safety: check-runner-collector-admission
check-runner-collector-admission:  ## Prove the root-only receiver boundary in an isolated network namespace
	@docker build --tag agentsfleet-runner-collector-admission-test --file scripts/fixtures/runner-collector-admission.Dockerfile scripts/fixtures
	@docker run --rm --network=none --cap-add=NET_ADMIN --env NO_PROXY=127.0.0.1,localhost --mount "type=bind,src=$(CURDIR),dst=/repository,readonly" agentsfleet-runner-collector-admission-test
