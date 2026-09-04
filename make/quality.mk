# =============================================================================
# QUALITY — code quality, formatting, analysis
# =============================================================================

.PHONY: check-cutover-probes lint-scripts _model_allowlist_check check-migrate-unprivileged lint-all lint-rustd lint-website lint-apps-designsystem-cli lint-app lint-design-system lint-cli lint-shell check-documentation-rules check-gh-actions-valid check-playbooks check-playbooks-refs

check-documentation-rules:  ## Check public API and command help text
	@PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_documentation_rules_test.py
	@PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_documentation_rules.py

ACTIONLINT ?= actionlint

lint-website:  ## Lint website only (Oxlint + tsc)
	@echo "→ [website] Running Oxlint + TypeScript check..."
	@cd ui/packages/website && bun run lint
	@cd ui/packages/website && bun run typecheck
	@echo "✓ [website] Lint passed"

lint-app:  ## Lint ui/packages/app only (Oxlint + tsc)
	@echo "→ [app] Running Oxlint + TypeScript check..."
	@cd ui/packages/app && bun run lint
	@cd ui/packages/app && bun run typecheck
	@echo "✓ [app] Lint passed"

lint-design-system:  ## Lint ui/packages/design-system only (Oxlint + tsc)
	@echo "→ [design-system] Running Oxlint + TypeScript check..."
	@cd ui/packages/design-system && bun run lint
	@echo "✓ [design-system] Lint passed"

lint-cli: check-documentation-rules  ## Lint agentsfleet CLI and its public text
	@echo "→ [agentsfleet] Oxlint + runtime/const audits + tsc..."
	@cd cli && bun run lint
	@echo "✓ [agentsfleet] Lint passed"

# Governance gates: the script-driven checks that enforce repository CONVENTIONS
# rather than compile correctness. Grouped under one target so `lint-zig` names a
# policy set instead of a growing list, and so a new rule extends this line
# rather than adding another near-duplicate wrapper.
#
# Deliberately NOT folded in: _fmt_check / _zlint_check (tooling, not policy) and
# check-test-reachability / _lint_zig_test_depth (test structure, and the latter
# is invoked directly to record a spec's test baseline).
_model_allowlist_check:
	@echo "→ [models] Checking every dialable provider is priced or carries a reason..."
	@python3 scripts/check_model_allowlist.py


# The Rust workspace. `cd` rather than `--manifest-path`: rust-toolchain.toml is
# resolved from the WORKING DIRECTORY, so running cargo from the repository root
# would silently use whatever toolchain the shell has active instead of the
# pinned one — the lane would pass on a compiler nobody agreed to.
#
# `--all-targets` covers tests and benches too. A clippy violation that only
# exists in a test file is still a violation; excluding them is how a test file
# becomes the place lint rules go to die.
RUSTD_DIR := rustd

lint-rustd:  ## Lint the Rust workspace (rustfmt + clippy, warnings are errors)
	@command -v cargo >/dev/null 2>&1 || { echo "✗ cargo not found. Install via: mise install rust"; exit 1; }
	@cd $(RUSTD_DIR) && $(WITH_PROGRESS) "[rustd] rustfmt --check" -- cargo fmt --check
	@# --all-features, not the default set: a crate's `test-util` feature gates
	@# its mockable input/output core (M-MOCKABLE-SYSCALLS), and without this
	@# flag that code is never compiled here — so the one module whose whole job
	@# is to be exercised by tests would be the one module lint never sees.
	@cd $(RUSTD_DIR) && $(WITH_PROGRESS) "[rustd] clippy -D warnings" -- \
	  cargo clippy --workspace --all-targets --all-features -- -D warnings
	@# The configuration that ships: no features at all. Every other lane turns
	@# `openapi` and `test-util` on, so an import that exists only for an
	@# annotation broke the production build once without any lane noticing.
	@cd $(RUSTD_DIR) && $(WITH_PROGRESS) "[rustd] check --bin agentsfleetd (no features)" -- \
	  cargo check -p agentsfleetd --bin agentsfleetd

# Every scripts/*_test.py, discovered rather than listed.
#
# A checker whose own tests never run is enforcement in appearance only, so the
# self-tests get their own lane rather than riding an unrelated one where a
# future edit can silently unhook them.
#
# `*_test.py`, not `check_*_test.py`: the narrower pattern would let a self-test
# be written, committed and never run.
SCRIPT_SELF_TESTS := python3 -m unittest discover -s scripts -t scripts -p '*_test.py'

lint-scripts:  ## Run every scripts/*_test.py self-test
	@echo "→ [scripts] Running script self-tests..."
	@$(SCRIPT_SELF_TESTS)
	@echo "✓ [scripts] Script self-tests passed"

# The cutover probe runner's own tests, plus its three asserts run for real
# against this repository. Hermetic — fixtures for the negatives, no daemon —
# so the row-coverage claim is graded at every `lint-all` rather than only on
# swap day, which is the one day nobody wants to discover it.
check-cutover-probes:  ## Assert cutover probe row-coverage + run the probe runner's self-tests
	@echo "→ [cutover] Probe runner self-tests..."
	@bash playbooks/operations/cutover/probes_test.sh
	@echo "→ [cutover] Row-coverage, rollback and architecture asserts..."
	@bash playbooks/operations/cutover/probes.sh --coverage
	@echo "✓ [cutover] Probe runner green"

SHELLCHECK ?= shellcheck

lint-shell:  ## Lint scripts/*.sh via shellcheck (follows dotfiles symlinks)
	@echo "→ [shell] Running shellcheck on scripts/*.sh..."
	@command -v $(SHELLCHECK) >/dev/null 2>&1 || { echo "shellcheck not found. Install via: mise install shellcheck"; exit 1; }
	@# `--severity=error` is the floor: catches genuine breakage (syntax,
	@# undefined-vars, dangerous quoting) without blocking on pre-existing
	@# stylistic warnings in symlinked dotfiles/scripts/. Tighten to
	@# `warning` once dotfiles cleanup lands.
	@# `-x` lets shellcheck follow `source`/`.` into sibling scripts.
	@$(SHELLCHECK) --severity=error -x scripts/*.sh
	@echo "✓ [shell] shellcheck passed (error-level)"

lint-apps-designsystem-cli: lint-app lint-design-system lint-cli  ## Lint app + design-system + agentsfleet






lint-all: lint-rustd lint-scripts _model_allowlist_check lint-website lint-apps-designsystem-cli lint-shell check-documentation-rules check-gh-actions-valid check-playbooks check-architecture-doc check-deploy-safety test-parity-self-test bench-cutover-self-test check-cutover-probes  ## Run all linters + quality gates
	@echo "✓ All lint checks passed"

check-gh-actions-valid:  ## Validate .github/workflows/ — actionlint (YAML + run: shellcheck) + action pins + make-target ref check
	@echo "→ [gh-actions] Running actionlint on workflows..."
	@command -v $(ACTIONLINT) >/dev/null 2>&1 || { echo "actionlint not found. Install via: mise install actionlint"; exit 1; }
	@$(ACTIONLINT) .github/workflows/*.yml
	@# actionlint validates the YAML and the `run:` shell; it has no opinion on
	@# whether a pin's runtime still exists or whether its ref can move. That is
	@# this script's half, and it rides the same target because both answer one
	@# question: will these workflows still work tomorrow.
	@echo "→ [gh-actions] Checking action pins — runtimes and mutable refs..."
	@bash audits/gh-actions-runtime.sh
	@# The builder pin lives in its own script so it can have its own tests —
	@# the shape check-architecture-doc uses. A gate with no self-test can pass
	@# vacuously: a broken grep reports a clean tree and reads like success.
	@echo "→ [gh-actions] Builder-image tag is derived, not pasted..."
	@bash scripts/check_builder_pin_test.sh
	@bash scripts/check_builder_pin.sh
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

check-migrate-unprivileged: _ensure-test-infra  ## Migrate from empty as a NON-superuser, the shape managed databases actually hand the migrator
	@bash scripts/check-migrate-unprivileged.sh

# Every deployment input the playbooks read off the ambient environment, dropped
# before each suite runs. The suites build their child environments additively,
# so without this they inherit the developer's shell and a run is only as
# reproducible as whatever that shell happens to export. Two have bitten:
# AGENTSFLEET_API_URL fails `runner_test.sh`'s ENV=prod case on any shell pointed
# at api-dev, and VAULT_DEV retargets the vault under `credentials_test.sh` so a
# negative test finds its "missing" input present. The rest are listed because
# they are the same kind of input, not because they break today — a suite that
# starts reading one should not reintroduce the trap. Continuous Integration (CI)
# invokes this target with none of them set (.github/workflows/lint.yml), so the
# scrub changes nothing there; it makes a developer's run match it.
PLAYBOOK_TEST_SCRUB = -u ENV -u STAGE -u ACTION -u PUSH -u REVISION \
  -u VAULT -u VAULT_DEV -u VAULT_PROD -u RUNNER_ITEM -u AGENTSFLEET_API_URL

# Split by what a change can actually break. The reference-integrity and
# README-parity halves are cheap greps over the tree and are the ONLY things a
# Makefile or workflow edit can invalidate (a make target citing a playbook
# path). The shellcheck + regression-test halves cost minutes and can only be
# invalidated by editing playbooks/ itself. .githooks/pre-commit routes
# accordingly, so a one-line make/*.mk change stops paying for 21 test suites.
check-playbooks: check-playbooks-refs check-vault-gate-parity  ## Validate playbooks/ — vault-gate parity + shellcheck + regression tests + reference integrity + README/tree parity
	@echo "→ [playbooks] shellcheck on playbooks/**/*.sh..."
	@command -v $(SHELLCHECK) >/dev/null 2>&1 || { echo "shellcheck not found. Install via: mise install shellcheck"; exit 1; }
	@find playbooks -name '*.sh' -print0 | xargs -0 $(SHELLCHECK) --severity=error -x
	@echo "→ [playbooks] focused shell regression tests (bounded parallel)..."
	@PLAYBOOK_TEST_SCRUB="$(PLAYBOOK_TEST_SCRUB)" bash scripts/run-playbook-tests.sh

check-playbooks-refs:  ## playbooks/ reference integrity + README parity only (cheap; what a Makefile edit can break)
	@echo "→ [playbooks] reference integrity — every playbooks/ path resolves..."
	@# Scans the live operational surface (CI, scripts, active docs, the playbooks
	@# themselves). Excludes docs/v2/: specs are historical records that
	@# intentionally cite now-moved paths.
	@FAIL=0; \
	REFS=$$(git grep -h -E 'playbooks/[A-Za-z0-9_./-]+' -- . ':(exclude)docs/v2/**' | \
	awk '{ text = $$0; while (match(text, /playbooks\/[A-Za-z0-9_.\/-]+/)) { ref = substr(text, RSTART, RLENGTH); sub(/[.,):]*$$/, "", ref); print ref; text = substr(text, RSTART + RLENGTH); } }' | sort -u); \
	if [ -z "$$REFS" ]; then echo "✗ [playbooks] reference scan matched nothing"; exit 1; fi; \
	for ref in $$REFS; do \
	  [ -e "$$ref" ] || { echo "✗ broken playbooks/ reference: $$ref"; FAIL=1; }; \
	done; \
	if [ $$FAIL -eq 1 ]; then echo "✗ [playbooks] reference integrity failed"; exit 1; fi; \
	echo "✓ [playbooks] all references resolve"
	@echo "→ [playbooks] README ↔ tree parity..."
	@ACTUAL=$$(mktemp); DOCUMENTED=$$(mktemp); \
	trap 'rm -f "$$ACTUAL" "$$DOCUMENTED"' EXIT; \
	find playbooks/founding playbooks/deploy playbooks/operations -type f -name '001_playbook.md' \
	  -exec dirname {} \; | sed 's|^playbooks/||' | sort > "$$ACTUAL"; \
	sed -n '/<!-- playbook-inventory:start -->/,/<!-- playbook-inventory:end -->/p' \
	  playbooks/README.md | sed -n 's/^- `\([^`]*\)`.*/\1/p' | sort > "$$DOCUMENTED"; \
	if [ ! -s "$$DOCUMENTED" ]; then echo "✗ [playbooks] README inventory is empty"; exit 1; fi; \
	if ! cmp -s "$$ACTUAL" "$$DOCUMENTED"; then \
	  echo "✗ [playbooks] README inventory differs from disk"; \
	  diff -u "$$DOCUMENTED" "$$ACTUAL" || true; \
	  exit 1; \
	fi; \
	echo "✓ [playbooks] README inventory exactly matches disk"
