# =============================================================================
# Safety gates — deploy correctness, vault-read approval, architecture-doc truth
# =============================================================================
#
# Three static checks that assert properties of the tree rather than build
# anything, like check-openapi or check-route-registration-doc. Split out of
# quality.mk (RULE FLL), which the vault-gate parity scan pushed past the
# 350-line cap; the same reason check-test-reachability.mk lives on its own.
#
# Where they fire: all three are prerequisites of `lint-all`, so CI's lint job
# runs them. check-vault-gate-parity additionally hangs off check-playbooks, so
# validating the playbooks surface can never skip the guardrail that protects it.

.PHONY: check-vault-gate-parity check-architecture-doc check-deploy-safety

check-vault-gate-parity:  ## Every playbooks/operations/ script that reads the vault calls both approval + auth gates
	@echo "→ [playbooks] vault-gate parity — every operations script that reads the vault passes both gates..."
	@# A vault reader is detected by the `op://` reference scheme, not the `op read`
	@# verb. Every read names an `op://…` ref, and the repo reads through several
	@# spellings — a literal `op read`, `op --account X read`, and the common.sh
	@# helper `playbooks_read_ref_or_empty` (no `op read` in the caller at all). The
	@# scheme is the one signal every reader shares, so matching it is immune to all
	@# three. Only whole-line comments are dropped (`^[[:space:]]*#`); a mid-line
	@# `#` inside a string is left intact rather than risk deleting the ref after it.
	@# The empty-scan guard mirrors check-playbooks' reference check — a scan that
	@# matched nothing has proved nothing, and would pass silently after a refactor.
	@FAIL=0; \
	READERS=$$(for f in $$(find playbooks/operations -name '*.sh' | sort); do \
	  grep -vE '^[[:space:]]*#' "$$f" | grep -qF 'op://' && echo "$$f"; \
	done); \
	if [ -z "$$READERS" ]; then echo "✗ [playbooks] vault-gate parity scan matched no vault readers — the scan is broken, not the tree"; exit 1; fi; \
	for f in $$READERS; do \
	  for sym in 'lib/common.sh' 'playbooks_require_vault_read_approval' 'playbooks_require_op_auth'; do \
	    grep -q "$$sym" "$$f" || { echo "✗ $$f reads the vault but never calls: $$sym"; FAIL=1; }; \
	  done; \
	done; \
	if [ $$FAIL -eq 1 ]; then echo "✗ [playbooks] vault-gate parity failed — add the common.sh preamble (see playbooks/operations/ip_allowlisting/01_egress_inventory.sh)"; exit 1; fi; \
	echo "✓ [playbooks] every vault-reading operations script passes both gates"
	@echo "→ [playbooks] vault-gate self-tests..."
	@bash playbooks/operations/credential_rotation/vault_gate_test.sh

check-architecture-doc:  ## docs/architecture/ stays true — milestone refs resolve, relative links resolve, no orphan markers
	@bash scripts/check_architecture_doc_test.sh
	@bash scripts/check_architecture_doc.sh

check-deploy-safety:  ## deploy.sh version-skip equality + deploy mutex, and shellcheck over deploy/baremetal/
	@# deploy/ sits outside _shell_lint's scripts/*.sh glob, so it would otherwise
	@# never be shellchecked. The two lock cases need flock (util-linux); they skip
	@# on a machine without it and hard-fail when CI is set — see deploy_test.sh.
	@command -v $(SHELLCHECK) >/dev/null 2>&1 || { echo "shellcheck not found. Install via: mise install shellcheck"; exit 1; }
	@$(SHELLCHECK) --severity=error -x deploy/baremetal/*.sh
	@bash deploy/baremetal/deploy_test.sh
