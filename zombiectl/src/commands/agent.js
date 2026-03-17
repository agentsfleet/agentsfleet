import { commandAgentScores } from "./agent_scores.js";
import { commandAgentProfile } from "./agent_profile.js";
import { commandAgentProposals } from "./agent_proposals.js";
import { commandAgentHarness } from "./agent_harness.js";
import { validateRequiredId } from "../program/validate.js";

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
    const check = validateRequiredId(agentId, "agent-id");
    if (!check.ok) {
      writeLine(ctx.stderr, ui.err(check.message));
      return 2;
    }
    return commandAgentScores(ctx, parsed, agentId, deps);
  }

  if (action === "profile") {
    if (!agentId) {
      writeLine(ctx.stderr, ui.err("agent profile requires <agent-id>"));
      return 2;
    }
    const check = validateRequiredId(agentId, "agent-id");
    if (!check.ok) {
      writeLine(ctx.stderr, ui.err(check.message));
      return 2;
    }
    return commandAgentProfile(ctx, parsed, agentId, deps);
  }

  if (action === "proposals") {
    if (!agentId) {
      writeLine(ctx.stderr, ui.err("agent proposals requires <agent-id>"));
      return 2;
    }
    const check = validateRequiredId(agentId, "agent-id");
    if (!check.ok) {
      writeLine(ctx.stderr, ui.err(check.message));
      return 2;
    }
    return commandAgentProposals(ctx, parsed, agentId, deps);
  }

  if (action === "harness") {
    if (!agentId) {
      writeLine(ctx.stderr, ui.err("agent harness revert requires <agent-id>"));
      return 2;
    }
    const check = validateRequiredId(agentId, "agent-id");
    if (!check.ok) {
      writeLine(ctx.stderr, ui.err(check.message));
      return 2;
    }
    const changeId = parsed.options["to-change"];
    if (!changeId) {
      writeLine(ctx.stderr, ui.err("agent harness revert requires --to-change <change-id>"));
      return 2;
    }
    const changeCheck = validateRequiredId(changeId, "change-id");
    if (!changeCheck.ok) {
      writeLine(ctx.stderr, ui.err(changeCheck.message));
      return 2;
    }
    return commandAgentHarness(ctx, parsed, agentId, deps);
  }

  writeLine(ctx.stderr, ui.err("usage: agent scores <agent-id> [--limit N] [--starting-after ID] [--json]"));
  writeLine(ctx.stderr, ui.err("       agent profile <agent-id> [--json]"));
  writeLine(ctx.stderr, ui.err("       agent proposals <agent-id> [approve <proposal-id> | reject <proposal-id> [--reason TEXT] | --json]"));
  writeLine(ctx.stderr, ui.err("       agent harness revert <agent-id> --to-change <change-id>"));
  return 2;
}
