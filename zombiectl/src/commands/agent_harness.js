import { AGENTS_PATH } from "../lib/api-paths.js";
import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";
import { writeError } from "../program/io.js";

export async function commandAgentHarness(ctx, parsed, agentId, deps) {
  const { request, apiHeaders, printJson, ui, writeLine } = deps;

  const subaction = parsed.positionals[0] || null;
  if (subaction !== "revert") {
    writeError(ctx, "UNKNOWN_COMMAND", "usage: agent harness revert <agent-id> --to-change <change-id>", deps);
    return 2;
  }

  const changeId = parsed.options["to-change"] || null;
  if (!changeId) {
    writeError(ctx, "USAGE_ERROR", "agent harness revert requires --to-change <change-id>", deps);
    return 2;
  }

  const res = await request(
    ctx,
    `${AGENTS_PATH}${encodeURIComponent(agentId)}/harness/changes/${encodeURIComponent(changeId)}:revert`,
    {
      method: "POST",
      headers: apiHeaders(ctx),
    },
  );
  setCliAnalyticsContext(ctx, {
    agent_id: agentId,
    change_id: res.change_id,
    reverted_from: res.reverted_from,
  });
  queueCliAnalyticsEvent(ctx, "agent_harness_reverted", {
    agent_id: agentId,
    change_id: res.change_id,
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    writeLine(ctx.stdout, ui.ok(`reverted ${res.reverted_from} -> ${res.change_id}`));
  }
  return 0;
}
