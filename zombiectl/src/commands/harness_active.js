import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";

export async function commandHarnessActive(ctx, parsed, workspaceId, deps) {
  const { request, apiHeaders, printJson, printKeyValue = () => {}, printSection = () => {} } = deps;

  const res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/harness/active`, {
    method: "GET",
    headers: apiHeaders(ctx),
  });

  setCliAnalyticsContext(ctx, {
    workspace_id: workspaceId,
    agent_id: res.agent_id ?? "default-v1",
    harness_config_version_id: res.config_version_id ?? "default-v1",
    run_snapshot_version: res.run_snapshot_version ?? "default-v1",
  });
  queueCliAnalyticsEvent(ctx, "harness_active_viewed", {
    workspace_id: workspaceId,
    agent_id: res.agent_id ?? "default-v1",
  });
  if (ctx.jsonMode) printJson(ctx.stdout, res);
  else {
    printSection(ctx.stdout, "Active harness");
    printKeyValue(ctx.stdout, {
      workspace_id: workspaceId,
      agent_id: res.agent_id ?? "default-v1",
      config_version_id: res.config_version_id ?? "default-v1",
      run_snapshot_version: res.run_snapshot_version ?? "default-v1",
    });
  }
  return 0;
}
