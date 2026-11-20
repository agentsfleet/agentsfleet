# Checks on the tree rather than the product. check-deploy-safety rides lint-all,
# check-release-candidate rides check-gh-actions-valid and check-vault-gate-parity
# rides check-playbooks, so CI's lint jobs run all three.

.PHONY: check-vault-gate-parity check-deploy-safety
.PHONY: check-release-candidate

check-gh-actions-valid: check-release-candidate

check-release-candidate: ## Prove candidate promotion rejects untrusted, mismatched and tampered artifacts
	@PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_release_candidate.py

check-vault-gate-parity:  ## Every playbooks/operations/ script that reads the vault calls both approval + auth gates
	@echo "→ [playbooks] vault-gate parity — every operations script that reads the vault passes both gates..."
	@# A vault reader is found by its `op://` reference, the one thing every read
	@# spelling shares; only whole-line comments are dropped. A scan that matched
	@# nothing proved nothing, so it fails.
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

check-deploy-safety:  ## shellcheck + every *_test.sh suite under deploy/baremetal/ + the Fly deploy proofs + the Docker receiver-admission proof
	@# deploy/ is outside lint-shell's glob. Suites are found by name; a case
	@# needing a Linux tool skips locally and hard-fails when CI is set.
	@command -v $(SHELLCHECK) >/dev/null 2>&1 || { echo "shellcheck not found. Install via: mise install shellcheck"; exit 1; }
	@$(SHELLCHECK) --severity=error -x deploy/baremetal/*.sh
	@PLAYBOOK_TEST_SCRUB="$(PLAYBOOK_TEST_SCRUB)" bash scripts/run-playbook-tests.sh deploy/baremetal
	@PYTHONDONTWRITEBYTECODE=1 python3 scripts/deploy_fly_test.py
