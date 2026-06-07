# =============================================================================
# HARNESS VERIFY — deterministic gate audits (the mechanical layer)
# =============================================================================
#
# `make harness-verify` runs every deterministic gate audit in one place.
# This is the mechanical layer of HARNESS VERIFY as described in AGENTS.md:
# each audit consumes the staged diff (pre-commit context) and exits 0/1
# without agent judgement.
#
# Visual identity — the cyan ● banner mirrors the design-system LIVE pulse
# (Operational Restraint). One emoji, used only when something is verified
# alive; everything else is monochrome chrome.
#
# Wiring:
#   .githooks/pre-commit invokes `make harness-verify` BEFORE `make lint`
#   when lint-relevant files are staged. Harness-verify is seconds-fast and
#   fails on the cheapest discipline regressions before paying for oxlint /
#   tsc / zlint / actionlint / redocly.
#
# Scope:
#   harness-verify (pre-commit) passes `--staged` to each per-file audit so it
#   judges only the files in the commit (`git diff --cached`); pre-existing
#   debt in untouched files does not block an unrelated commit. `--staged`
#   reads the index, so a fix staged but not yet committed satisfies the check
#   on the same hook run (no BASE...HEAD blindness — the M70 concern).
#
#   Canonical full-codebase enforcement lives in harness-verify-all (below)
#   and `make lint` / CI — the audits default to full-codebase via
#   `git ls-files`; only this pre-commit target opts into `--staged`.
#
#   cross-tier-rates.sh always checks its fixed rate-pin file set (no scope
#   flag). msid-ui.sh is diff-shaped by construction and stays on `--staged`.
#
#   M68 commit 02c1f3cf (the orphan-cleanup slip) was the forcing
#   function — pre-commit `HEAD` is the prior commit, so a `BASE...HEAD`
#   check was blind to a fix the agent staged but had not yet committed.
#
# Adding a gate:
#   1. Drop audits/<gate>.sh on disk (or symlink from dotfiles).
#   2. Add a row in HARNESS_GATES below with the gate's short label + the
#      command that runs the audit.
#   3. Update docs/gates/<gate>.md with "Fires in: make harness-verify".
#   4. Update dotfiles AGENTS.md HARNESS_KEYS array.

.PHONY: harness-verify harness-verify-all

# ANSI colour codes — only emitted to TTY. The MAKE_TERMOUT trick lets CI
# (which redirects stdout) get plain text.
C_CYAN   := \033[36m
C_GREEN  := \033[32m
C_RED    := \033[31m
C_YELLOW := \033[33m
C_GREY   := \033[2m
C_BOLD   := \033[1m
C_RESET  := \033[0m

# ── Gate registry ──────────────────────────────────────────────────────────
# Format: <label>|<command>. Label is left-padded to align the column.
# Order: cheapest → most expensive so the fast lane fails fast.
define HARNESS_RUN
@printf "  $(C_GREY)→$(C_RESET) %-20s " "$(1)"; \
if out=$$($(2) 2>&1); then \
  summary=$$(printf '%s\n' "$$out" | tail -1); \
  printf "$(C_GREEN)✓$(C_RESET) $(C_GREY)%s$(C_RESET)\n" "$$summary"; \
else \
  printf "$(C_RED)✗$(C_RESET)\n"; \
  printf '%s\n' "$$out" | sed 's/^/      /'; \
  exit 1; \
fi
endef

harness-verify:  ## Run every deterministic gate audit (mechanical HARNESS VERIFY layer; staged scope — pre-commit lens)
	@printf "\n$(C_BOLD)$(C_CYAN)●$(C_RESET) $(C_BOLD)HARNESS VERIFY$(C_RESET) $(C_GREY)── deterministic gates · staged scope (pre-commit lens)$(C_RESET)\n"
	$(call HARNESS_RUN,UFS,audits/ufs.sh --staged)
	$(call HARNESS_RUN,DESIGN TOKEN,audits/design-tokens.sh --staged)
	$(call HARNESS_RUN,SPEC TEMPLATE,audits/spec-template.sh --staged)
	$(call HARNESS_RUN,ERROR REGISTRY,audits/error-codes.sh --staged)
	$(call HARNESS_RUN,LOGGING,audits/logging.sh --staged)
	$(call HARNESS_RUN,LIFECYCLE,audits/deinit-pairs.sh --staged)
	$(call HARNESS_RUN,CROSS-TIER RATES,audits/cross-tier-rates.sh)
	$(call HARNESS_RUN,MS-ID + UI,audits/msid-ui.sh --staged)
	@printf "$(C_BOLD)$(C_CYAN)●$(C_RESET) $(C_BOLD)$(C_GREEN)ALL GATES GREEN$(C_RESET) $(C_GREY)── ready for VERIFY$(C_RESET)\n\n"

harness-verify-all:  ## Whole-worktree variant for periodic deep audits
	@printf "\n$(C_BOLD)$(C_CYAN)●$(C_RESET) $(C_BOLD)HARNESS VERIFY$(C_RESET) $(C_GREY)── deterministic gates · whole worktree$(C_RESET)\n"
	# After M70 every audit defaults to full-codebase, so harness-verify-all
	# differs from harness-verify only in the COMBINED check's scope:
	# `--diff` (vs origin/main) is the broadest meaningful scope for that
	# diff-shaped script.
	$(call HARNESS_RUN,UFS,audits/ufs.sh)
	$(call HARNESS_RUN,DESIGN TOKEN,audits/design-tokens.sh)
	$(call HARNESS_RUN,SPEC TEMPLATE,audits/spec-template.sh)
	$(call HARNESS_RUN,ERROR REGISTRY,audits/error-codes.sh)
	$(call HARNESS_RUN,LOGGING,audits/logging.sh)
	$(call HARNESS_RUN,LIFECYCLE,audits/deinit-pairs.sh)
	$(call HARNESS_RUN,CROSS-TIER RATES,audits/cross-tier-rates.sh)
	$(call HARNESS_RUN,MS-ID + UI,audits/msid-ui.sh --diff)
	@printf "$(C_BOLD)$(C_CYAN)●$(C_RESET) $(C_BOLD)$(C_GREEN)ALL GATES GREEN$(C_RESET) $(C_GREY)── whole-worktree sweep clean$(C_RESET)\n\n"
