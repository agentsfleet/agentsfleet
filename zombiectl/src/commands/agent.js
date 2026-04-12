import { setCliAnalyticsContext } from "../lib/analytics.js";
import { commandAgentScores } from "./agent_scores.js";
import { commandAgentProfile } from "./agent_profile.js";
import { commandAgentImprovementReport } from "./agent_improvement_report.js";
import { commandAgentProposals } from "./agent_proposals.js";
import { commandAgentHarness } from "./agent_harness.js";
import { commandAgentCreate, commandAgentList, commandAgentDelete } from "./agent_external.js";
import { validateRequiredId } from "../program/validate.js";
import { writeError } from "../program/io.js";

export async function commandAgent(ctx, args, workspaces, deps) {
  const { parseFlags, ui, writeLine } = deps;

  const action = args[0];
  const parsed = parseFlags(args.slice(1));
  const agentId = parsed.positionals[0] || parsed.options["agent-id"];
  if (agentId) setCliAnalyticsContext(ctx, { agent_id: agentId });

  if (action === "scores") {
    if (!agentId) {
      writeError(ctx, "USAGE_ERROR", "agent scores requires <agent-id>", deps);
      return 2;
    }
    const check = validateRequiredId(agentId, "agent-id");
    if (!check.ok) {
      writeError(ctx, "VALIDATION_ERROR", check.message, deps);
      return 2;
    }
    return commandAgentScores(ctx, parsed, agentId, deps);
  }

  if (action === "profile") {
    if (!agentId) {
      writeError(ctx, "USAGE_ERROR", "agent profile requires <agent-id>", deps);
      return 2;
    }
    const check = validateRequiredId(agentId, "agent-id");
    if (!check.ok) {
      writeError(ctx, "VALIDATION_ERROR", check.message, deps);
      return 2;
    }
    return commandAgentProfile(ctx, parsed, agentId, deps);
  }

  if (action === "improvement-report") {
    if (!agentId) {
      writeError(ctx, "USAGE_ERROR", "agent improvement-report requires <agent-id>", deps);
      return 2;
    }
    const check = validateRequiredId(agentId, "agent-id");
    if (!check.ok) {
      writeError(ctx, "VALIDATION_ERROR", check.message, deps);
      return 2;
    }
    return commandAgentImprovementReport(ctx, parsed, agentId, deps);
  }

  if (action === "proposals") {
    if (!agentId) {
      writeError(ctx, "USAGE_ERROR", "agent proposals requires <agent-id>", deps);
      return 2;
    }
    const check = validateRequiredId(agentId, "agent-id");
    if (!check.ok) {
      writeError(ctx, "VALIDATION_ERROR", check.message, deps);
      return 2;
    }
    return commandAgentProposals(ctx, parsed, agentId, deps);
  }

  // M9_001: External agent key management
  if (action === "create") return commandAgentCreate(ctx, parsed, deps);
  if (action === "list")   return commandAgentList(ctx, parsed, deps);
  if (action === "delete") return commandAgentDelete(ctx, parsed, deps);

  if (action === "harness") {
    if (!agentId) {
      writeError(ctx, "USAGE_ERROR", "agent harness revert requires <agent-id>", deps);
      return 2;
    }
    const check = validateRequiredId(agentId, "agent-id");
    if (!check.ok) {
      writeError(ctx, "VALIDATION_ERROR", check.message, deps);
      return 2;
    }
    const changeId = parsed.options["to-change"];
    if (!changeId) {
      writeError(ctx, "USAGE_ERROR", "agent harness revert requires --to-change <change-id>", deps);
      return 2;
    }
    const changeCheck = validateRequiredId(changeId, "change-id");
    if (!changeCheck.ok) {
      writeError(ctx, "VALIDATION_ERROR", changeCheck.message, deps);
      return 2;
    }
    return commandAgentHarness(ctx, parsed, agentId, deps);
  }

  // non-JSON: preserve multi-line usage text not expressible as a single message
  if (ctx.jsonMode) {
    writeError(ctx, "UNKNOWN_COMMAND", `unknown agent subcommand: ${action}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err("usage: agent scores <agent-id> [--limit N] [--starting-after ID] [--json]"));
    writeLine(ctx.stderr, ui.err("       agent profile <agent-id> [--json]"));
    writeLine(ctx.stderr, ui.err("       agent improvement-report <agent-id> [--json]"));
    writeLine(ctx.stderr, ui.err("       agent proposals <agent-id> [approve|reject|veto <proposal-id>]"));
    writeLine(ctx.stderr, ui.err("       agent harness revert <agent-id> --to-change <change-id>"));
    writeLine(ctx.stderr, ui.err("       agent create --workspace <ws> --zombie <id> --name <name>"));
    writeLine(ctx.stderr, ui.err("       agent list   --workspace <ws>"));
    writeLine(ctx.stderr, ui.err("       agent delete --workspace <ws> <agent-id>"));
  }
  return 2;
}
