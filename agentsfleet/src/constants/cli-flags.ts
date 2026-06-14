/**
 * CLI option / flag names — the keys the parser stores in
 * `parsed.options[key]`. Centralised so a rename surfaces as one diff
 * across every reader instead of silently drifting per command.
 *
 * Naming: the constant matches the on-the-wire flag name exactly.
 * `OPT_WORKSPACE_ID = "workspace-id"` reflects `--workspace-id`.
 *
 * RULE UFS.
 */

export const OPT_WORKSPACE = "workspace";
export const OPT_WORKSPACE_ID = "workspace-id";
export const OPT_ZOMBIE = "zombie";
export const OPT_AGENTSFLEET_ID = "zombie-id";
export const OPT_AGENT_ID = "agent-id";
export const OPT_NAME = "name";
export const OPT_DESCRIPTION = "description";
export const OPT_FROM = "from";
export const OPT_TTY = "tty";
