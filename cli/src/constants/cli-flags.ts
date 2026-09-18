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
export const OPT_AGENT = "fleet";
export const OPT_FLEET_ID = "fleet-id";
export const OPT_AGENT_ID = "fleet-id";
export const OPT_NAME = "name";
export const OPT_DESCRIPTION = "description";
export const OPT_FROM = "from";
export const OPT_TTY = "tty";
export const OPT_LIBRARY = "library";
export const OPT_GITHUB = "github";
export const OPT_TEMPLATE = "template";
export const OPT_REF = "ref";
export const OPT_REPLACE = "replace";

/**
 * One spelling for the Fleet-library identifier, wherever it is shown to a
 * person: the option metavar, the install hint, and the empty-state line. It
 * read three different ways before this constant existed — `<library>` in the
 * gallery hint, `<library_id>` in the status empty state, and `<id>` in the
 * option — for one value.
 */
export const LIBRARY_ID_PLACEHOLDER = "<library_id>";
