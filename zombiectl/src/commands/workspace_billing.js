export async function commandWorkspaceUpgradeScale(ctx, parsed, workspaceId, deps) {
  const { request, apiHeaders, ui, printJson, writeLine } = deps;
  const subscriptionId = parsed.options["subscription-id"] || parsed.positionals[1];

  if (!subscriptionId) {
    writeLine(ctx.stderr, ui.err("workspace upgrade-scale requires --subscription-id"));
    return 2;
  }

  const res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/billing/scale`, {
    method: "POST",
    headers: apiHeaders(ctx),
    body: JSON.stringify({ subscription_id: subscriptionId }),
  });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    writeLine(ctx.stdout, ui.ok(`workspace upgraded to ${res.plan_tier}`));
    writeLine(ctx.stdout, `workspace_id: ${workspaceId}`);
    writeLine(ctx.stdout, `plan_tier: ${res.plan_tier}`);
    writeLine(ctx.stdout, `billing_status: ${res.billing_status}`);
    if (res.subscription_id != null) writeLine(ctx.stdout, `subscription_id: ${res.subscription_id}`);
  }
  return 0;
}
