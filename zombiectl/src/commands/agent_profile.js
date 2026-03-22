import { AGENTS_PATH } from "../lib/api-paths.js";
import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";

export async function commandAgentProfile(ctx, parsed, agentId, deps) {
  const { request, apiHeaders, printJson, printKeyValue, printSection = () => {} } = deps;

  const res = await request(ctx, `${AGENTS_PATH}${encodeURIComponent(agentId)}`, {
    method: "GET",
    headers: apiHeaders(ctx),
  });
  setCliAnalyticsContext(ctx, {
    agent_id: res.agent_id,
    workspace_id: res.workspace_id,
    trust_level: res.trust_level,
    agent_status: res.status,
  });
  queueCliAnalyticsEvent(ctx, "agent_profile_viewed", {
    agent_id: res.agent_id,
    workspace_id: res.workspace_id,
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    printSection(ctx.stdout, `Agent profile · ${res.agent_id}`);
    printKeyValue(ctx.stdout, {
      agent_id: res.agent_id,
      name: res.name,
      status: res.status,
      workspace_id: res.workspace_id,
      trust_level: res.trust_level,
      trust_streak_runs: res.trust_streak_runs,
      improvement_stalled_warning: res.improvement_stalled_warning,
      created_at: res.created_at,
      updated_at: res.updated_at,
    });
  }
  return 0;
}
