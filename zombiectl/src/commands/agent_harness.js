import { AGENTS_PATH } from "../lib/api-paths.js";

export async function commandAgentHarness(ctx, parsed, agentId, deps) {
  const { request, apiHeaders, printJson, ui, writeLine } = deps;

  const subaction = parsed.positionals[0] || null;
  if (subaction !== "revert") {
    writeLine(ctx.stderr, ui.err("usage: agent harness revert <agent-id> --to-change <change-id>"));
    return 2;
  }

  const changeId = parsed.options["to-change"] || null;
  if (!changeId) {
    writeLine(ctx.stderr, ui.err("agent harness revert requires --to-change <change-id>"));
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

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    writeLine(ctx.stdout, ui.ok(`reverted ${res.reverted_from} -> ${res.change_id}`));
  }
  return 0;
}
