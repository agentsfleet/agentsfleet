import { AGENTS_PATH } from "../lib/api-paths.js";

export async function commandAgentProfile(ctx, parsed, agentId, deps) {
  const { request, apiHeaders, ui, printJson, printKeyValue, writeLine } = deps;

  const res = await request(ctx, `${AGENTS_PATH}${encodeURIComponent(agentId)}`, {
    method: "GET",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    printKeyValue(ctx.stdout, {
      agent_id: res.agent_id,
      name: res.name,
      status: res.status,
      workspace_id: res.workspace_id,
      created_at: res.created_at,
      updated_at: res.updated_at,
    });
  }
  return 0;
}
