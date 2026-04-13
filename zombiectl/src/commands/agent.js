// M9_001 §6.0 — External agent key management CLI.
// Replaces the old agent profile/scores CLI (removed in M17_001).
//
// zombiectl agent create --workspace <ws> --zombie <id> --name <name>
// zombiectl agent list   --workspace <ws>
// zombiectl agent delete --workspace <ws> <agent_id>

import { commandAgentCreate, commandAgentList, commandAgentDelete } from "./agent_external.js";
import { writeError } from "../program/io.js";

export async function commandAgent(ctx, args, _workspaces, deps) {
  const { parseFlags, ui, writeLine } = deps;

  const action = args[0];
  const parsed = parseFlags(args.slice(1));

  if (action === "create") return commandAgentCreate(ctx, parsed, deps);
  if (action === "list")   return commandAgentList(ctx, parsed, deps);
  if (action === "delete") return commandAgentDelete(ctx, parsed, deps);

  if (ctx.jsonMode) {
    writeError(ctx, "UNKNOWN_COMMAND", `unknown agent subcommand: ${action ?? "(none)"}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err("usage: zombiectl agent create --workspace <ws> --zombie <id> --name <name>"));
    writeLine(ctx.stderr, ui.err("       zombiectl agent list   --workspace <ws>"));
    writeLine(ctx.stderr, ui.err("       zombiectl agent delete --workspace <ws> <agent-id>"));
  }
  return 2;
}
