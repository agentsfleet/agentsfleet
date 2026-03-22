import { AGENTS_PATH } from "../lib/api-paths.js";
import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";

export async function commandAgentImprovementReport(ctx, parsed, agentId, deps) {
  const { request, apiHeaders, printJson, printKeyValue, printSection = () => {} } = deps;

  const res = await request(ctx, `${AGENTS_PATH}${encodeURIComponent(agentId)}/improvement-report`, {
    method: "GET",
    headers: apiHeaders(ctx),
  });
  setCliAnalyticsContext(ctx, {
    agent_id: res.agent_id,
    trust_level: res.trust_level,
    proposals_generated: res.proposals_generated,
    proposals_applied: res.proposals_applied,
    avg_score_delta_per_applied_change: res.avg_score_delta_per_applied_change,
  });
  queueCliAnalyticsEvent(ctx, "agent_improvement_report_viewed", {
    agent_id: res.agent_id,
    proposals_generated: res.proposals_generated,
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    printSection(ctx.stdout, `Agent improvement report · ${res.agent_id}`);
    printKeyValue(ctx.stdout, {
      agent_id: res.agent_id,
      trust_level: res.trust_level,
      improvement_stalled_warning: res.improvement_stalled_warning,
      proposals_generated: res.proposals_generated,
      proposals_approved: res.proposals_approved,
      proposals_vetoed: res.proposals_vetoed,
      proposals_rejected: res.proposals_rejected,
      proposals_applied: res.proposals_applied,
      avg_score_delta_per_applied_change: res.avg_score_delta_per_applied_change,
      current_tier: res.current_tier,
      baseline_tier: res.baseline_tier,
    });
  }
  return 0;
}
