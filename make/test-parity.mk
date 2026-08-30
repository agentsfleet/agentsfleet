# =============================================================================
# TEST-PARITY — the black-box HTTP contract lane, parameterised by base URL
# =============================================================================
# The cutover swaps which daemon serves the API. Nothing in either tree can
# answer "do these two answer the same way" — only a caller outside both can,
# and that is what this lane is.
#
# The harness is `scripts/parity_lane.sh`. It is SHELL, and deliberately not a
# Rust crate: a crate under `rustd/crates/` joins the 100%-line coverage flag
# and pays that rent for the life of the repository, for code whose whole job is
# pointing curl at two daemons.
#
# It is also NEW code rather than a repointed Zig suite. Of the 145 files in the
# Zig integration corpus, three speak HTTP; the rest import Zig modules and call
# them directly. Pointing that corpus at a Rust-served environment would still
# exercise Zig handler code and report a pass rate for the implementation being
# retired — worse than no number, because it reads like evidence.
#
# Two modes, both driven by the same roster:
#
#   BASE_URL alone                 RECORD — every route the contract declares
#                                  answers, and none answers 404. This is the
#                                  proof that the image serves the route table
#                                  the contract describes (rubric R3).
#
#   BASE_URL + COMPARE_URL         COMPARE — the same roster against both,
#                                  diffed per route × method. Any difference in
#                                  status, contract header or normalised body
#                                  fails naming the route and the method.
#
# The roster is REFLECTION over `public/openapi.json`, never a hand-kept list —
# a forgotten route is exactly the drift this lane exists to catch, and a list
# is the thing somebody forgets to update.

.PHONY: test-parity test-parity-self-test

PARITY_LANE := scripts/parity_lane.sh

test-parity:  ## Black-box HTTP contract lane (BASE_URL=<url> [COMPARE_URL=<url>])
	@if [ -z "$(BASE_URL)" ]; then \
	  echo "✗ [parity] BASE_URL is unset — the lane has nothing to probe."; \
	  echo "  Record one daemon:   make test-parity BASE_URL=http://127.0.0.1:3000"; \
	  echo "  Compare two:         make test-parity BASE_URL=http://127.0.0.1:8080 COMPARE_URL=http://127.0.0.1:3000"; \
	  exit 1; \
	fi
	@echo "→ [parity] Probing the contract roster against $(BASE_URL)$(if $(COMPARE_URL), and $(COMPARE_URL),)..."
	@BASE_URL="$(BASE_URL)" COMPARE_URL="$(COMPARE_URL)" bash $(PARITY_LANE)
	@echo "✓ [parity] Contract parity holds"

# The harness's own tests. Hermetic — fixture roster, fixture responder, no
# network and no daemon — so it rides `lint-all` rather than waiting for an
# environment. It is what proves the differ actually differs; without it the
# lane could pass every route by comparing nothing.
test-parity-self-test:  ## Run scripts/parity_lane_test.sh — the parity harness's own tests
	@echo "→ [parity] Running parity harness self-tests..."
	@bash scripts/parity_lane_test.sh
	@echo "✓ [parity] Parity harness self-tests passed"
