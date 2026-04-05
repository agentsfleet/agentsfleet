// M28_001 §5.3: zombiectl workspace billing --workspace-id <id>
// Shows billing breakdown for a workspace.

export async function commandWorkspaceBillingSummary(ctx, parsed, workspaceId, deps) {
  const { request, apiHeaders, ui, printJson, writeLine } = deps;
  const period = parsed.options["period"] || "30d";

  const url = `/v1/workspaces/${encodeURIComponent(workspaceId)}/billing/summary?period=${encodeURIComponent(period)}`;
  const res = await request(ctx, url, {
    method: "GET",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
    return 0;
  }

  const completed = res.completed || { count: 0, agent_seconds: 0 };
  const nonBillable = res.non_billable || { count: 0 };
  const scoreGated = res.non_billable_score_gated || { count: 0, avg_score: 0 };
  const total = res.total_runs || 0;

  writeLine(ctx.stdout, `workspace: ${workspaceId}  period: last ${res.period_days || 30} days`);
  writeLine(ctx.stdout, "");
  writeLine(ctx.stdout, `  completed (billed):          ${pad(completed.count)}    agent_seconds: ${fmt(completed.agent_seconds)}`);
  writeLine(ctx.stdout, `  non-billable:                ${pad(nonBillable.count)}`);
  if (scoreGated.count > 0) {
    writeLine(ctx.stdout, `  non-billable / score-gated:  ${pad(scoreGated.count)}    avg score: ${scoreGated.avg_score}`);
  } else {
    writeLine(ctx.stdout, `  non-billable / score-gated:  ${pad(0)}`);
  }
  writeLine(ctx.stdout, "  " + "\u2500".repeat(35));
  writeLine(ctx.stdout, `  total runs:                  ${pad(total)}`);

  return 0;
}

function pad(n) {
  return String(n).padStart(4);
}

function fmt(n) {
  if (n == null) return "0";
  return String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
