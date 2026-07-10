# =============================================================================
# check-test-reachability — every Zig `test` block runs, or is provably waived
# =============================================================================
#
# A static check, not a test lane: it compiles nothing you ship and asserts a
# property of the source tree, like check-openapi or check-route-registration-doc.
# Zig registers a file's `test` blocks only when the file is force-referenced at
# comptime from a test root; a plain `@import` registers nothing, so a block can
# sit on disk for months and never compile. Split out of quality.mk (RULE FLL).
#
# Where it fires: `lint-zig` (so CI's lint job and the pre-commit zig lane run it),
# and `_lint_zig_test_depth`, which `test-unit-agentsfleetd` invokes — that is the
# push-time path, since `.githooks/pre-push` runs the unit lane for any pushed
# `*.zig`, which is exactly when the test files have been written.

.PHONY: check-test-reachability _lint_zig_test_depth

REACHABLE_COUNTS := .tmp/zig-reachable-counts.txt
REACHABILITY_TESTS := python3 -m unittest discover -s scripts -t scripts -p 'check_zig_test_reachability*_test.py'

check-test-reachability:  ## Every Zig `test` block compiles from a test root, or carries a waiver
	@mkdir -p .tmp
	@echo "→ [zig] Checking every test block is reachable from a test root..."
	@$(REACHABILITY_TESTS) >/dev/null 2>&1 || \
	  { echo "✗ [zig] reachability checker self-tests failed"; $(REACHABILITY_TESTS); exit 1; }
	@python3 scripts/check_zig_test_reachability.py --check --counts-out $(REACHABLE_COUNTS)

# Counts come from the compiler-registered set, never a textual scan: a `test` block
# no test root force-imports never compiles, and crediting it would let VERIFY's Test
# Delta report growth that does not exist. Listing all 8 binaries costs ~10s, so this
# consumes what the reachability check already produced. `set -eu` + the numeric guard
# are load-bearing: without them an errored checker yields an empty count, `[ "" -lt
# 25 ]` errors instead of failing, and the recipe prints success and exits 0.
_lint_zig_test_depth: check-test-reachability
	@set -eu; \
	 counts=$$(cat $(REACHABLE_COUNTS)); \
	 unit_count=$$(printf '%s\n' "$$counts" | sed -n 's/^reachable_test_cases=//p'); \
	 integration_count=$$(printf '%s\n' "$$counts" | sed -n 's/^reachable_integration_cases=//p'); \
	 case "$$unit_count" in ''|*[!0-9]*) echo "✗ [zig] depth gate: bad unit count '$$unit_count'"; exit 1;; esac; \
	 case "$$integration_count" in ''|*[!0-9]*) echo "✗ [zig] depth gate: bad integration count '$$integration_count'"; exit 1;; esac; \
	 printf 'agentsfleetd_test_cases=%s\nagentsfleetd_integration_cases=%s\n' "$$unit_count" "$$integration_count" | tee .tmp/agentsfleetd-test-depth.txt >/dev/null; \
	 if [ "$$unit_count" -lt 25 ]; then echo "✗ expected at least 25 Zig tests, got $$unit_count"; exit 1; fi; \
	 if [ "$$integration_count" -lt 3 ]; then echo "✗ expected at least 3 Zig integration tests, got $$integration_count"; exit 1; fi; \
	 echo "✓ [zig] test depth gate passed (unit=$$unit_count integration=$$integration_count)"
