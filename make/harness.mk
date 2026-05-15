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
#   Each script's "diff scope" flag is passed so the audit operates on the
#   staged delta (not the whole repo). Flag conventions differ per script —
#   `audit-ufs.sh` post-M70 only supports `--all` (the `--diff` mode was
#   retired); `audit-design-tokens.sh` and `audit-combined.sh` accept both
#   `--diff` and `--staged`, and the pre-commit context wants `--staged` so
#   that staged-but-uncommitted changes are seen. The rest of the audits
#   use `--staged`. The flag passed to each call below matches what's
#   correct for the pre-commit invocation, not the script's *default*.
#
# Adding a gate:
#   1. Drop scripts/audit-<gate>.sh on disk (or symlink from dotfiles).
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

harness-verify:  ## Run every deterministic gate audit (mechanical HARNESS VERIFY layer; --staged)
	@printf "\n$(C_BOLD)$(C_CYAN)●$(C_RESET) $(C_BOLD)HARNESS VERIFY$(C_RESET) $(C_GREY)── deterministic gates · staged scope$(C_RESET)\n"
	$(call HARNESS_RUN,UFS,scripts/audit-ufs.sh --all)
	$(call HARNESS_RUN,DESIGN TOKEN,scripts/audit-design-tokens.sh --staged)
	$(call HARNESS_RUN,SPEC TEMPLATE,scripts/audit-spec-template.sh --staged)
	$(call HARNESS_RUN,ERROR REGISTRY,scripts/audit-error-codes.sh --staged)
	$(call HARNESS_RUN,LOGGING,scripts/audit-logging.sh --staged)
	$(call HARNESS_RUN,LIFECYCLE,scripts/audit-deinit-pairs.sh --staged)
	$(call HARNESS_RUN,COMBINED,scripts/audit-combined.sh --staged)
	@printf "$(C_BOLD)$(C_CYAN)●$(C_RESET) $(C_BOLD)$(C_GREEN)ALL GATES GREEN$(C_RESET) $(C_GREY)── ready for VERIFY$(C_RESET)\n\n"

harness-verify-all:  ## Whole-worktree variant for periodic deep audits
	@printf "\n$(C_BOLD)$(C_CYAN)●$(C_RESET) $(C_BOLD)HARNESS VERIFY$(C_RESET) $(C_GREY)── deterministic gates · whole worktree$(C_RESET)\n"
	$(call HARNESS_RUN,UFS,scripts/audit-ufs.sh --all)
	$(call HARNESS_RUN,DESIGN TOKEN,scripts/audit-design-tokens.sh --all)
	$(call HARNESS_RUN,SPEC TEMPLATE,scripts/audit-spec-template.sh --all)
	$(call HARNESS_RUN,ERROR REGISTRY,scripts/audit-error-codes.sh --all)
	$(call HARNESS_RUN,LOGGING,scripts/audit-logging.sh --all)
	$(call HARNESS_RUN,LIFECYCLE,scripts/audit-deinit-pairs.sh --all)
	# COMBINED is diff-shaped by construction — it asserts on *added* lines
	# (`^\+` in a unified diff), not on file state. There's no "whole-worktree"
	# semantic that makes sense; --diff (vs origin/main) is the broadest
	# meaningful scope. The script intentionally rejects --all (exit 2).
	$(call HARNESS_RUN,COMBINED,scripts/audit-combined.sh --diff)
	@printf "$(C_BOLD)$(C_CYAN)●$(C_RESET) $(C_BOLD)$(C_GREEN)ALL GATES GREEN$(C_RESET) $(C_GREY)── whole-worktree sweep clean$(C_RESET)\n\n"
