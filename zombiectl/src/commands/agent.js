import { commandAgentScores } from "./agent_scores.js";
import { commandAgentProfile } from "./agent_profile.js";

export async function commandAgent(ctx, args, workspaces, deps) {
  const { parseFlags, ui, writeLine } = deps;

  const action = args[0];
  const parsed = parseFlags(args.slice(1));
  const agentId = parsed.positionals[0] || parsed.options["agent-id"];

  if (action === "scores") {
    if (!agentId) {
      writeLine(ctx.stderr, ui.err("agent scores requires <agent-id>"));
      return 2;
    }
    return commandAgentScores(ctx, parsed, agentId, deps);
  }

  if (action === "profile") {
    if (!agentId) {
      writeLine(ctx.stderr, ui.err("agent profile requires <agent-id>"));
      return 2;
    }
    return commandAgentProfile(ctx, parsed, agentId, deps);
  }

  writeLine(ctx.stderr, ui.err("usage: agent scores <agent-id> [--limit N] [--starting-after ID] [--json]"));
  writeLine(ctx.stderr, ui.err("       agent profile <agent-id> [--json]"));
  return 2;
}
