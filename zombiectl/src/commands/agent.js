// External agent key management CLI.
//
// zombiectl agent add    --workspace <ws> --zombie <id> --name <name>
// zombiectl agent list   --workspace <ws>
// zombiectl agent delete --workspace <ws> <agent_id>

import { commandAgentAdd, commandAgentList, commandAgentDelete } from "./agent_external.js";
import { writeError } from "../program/io.js";
import { AUTH_PRESET, compose } from "../lib/error-map-presets.js";

// Agent commands hit /v1/workspaces/{ws}/agent-keys (POST/GET/DELETE).
// Server-side these can surface validation, conflict on duplicate
// names, and not-found on delete. The codes are not yet stabilized in
// OpenAPI for this surface, so we lean on AUTH_PRESET for now and
// expand as the audit (§4) flags missing entries.
export const errorMap = compose(AUTH_PRESET);

export async function commandAgent(ctx, args, _workspaces, deps) {
  const { parseFlags, ui, writeLine } = deps;

  const action = args[0];
  const parsed = parseFlags(args.slice(1));

  if (action === "add")    return commandAgentAdd(ctx, parsed, deps);
  if (action === "list")   return commandAgentList(ctx, parsed, deps);
  if (action === "delete") return commandAgentDelete(ctx, parsed, deps);

  if (ctx.jsonMode) {
    writeError(ctx, "UNKNOWN_COMMAND", `unknown agent subcommand: ${action ?? "(none)"}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err("usage: zombiectl agent add    --workspace <ws> --zombie <id> --name <name>"));
    writeLine(ctx.stderr, ui.err("       zombiectl agent list   --workspace <ws>"));
    writeLine(ctx.stderr, ui.err("       zombiectl agent delete --workspace <ws> <agent-id>"));
  }
  return 2;
}
