import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";

export async function commandHarnessActivate(ctx, parsed, workspaceId, deps) {
  const {
    request,
    apiHeaders,
    ui,
    printJson,
    printKeyValue = () => {},
    printSection = () => {},
    writeLine,
  } = deps;

  const profileVersionId = parsed.options["config-version-id"];
  if (!profileVersionId) {
    writeLine(ctx.stderr, ui.err("harness activate requires --config-version-id"));
    return 2;
  }

  const body = {
    config_version_id: profileVersionId,
    activated_by: parsed.options["activated-by"] || "zombiectl",
  };

  const res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/harness/activate`, {
    method: "POST",
    headers: apiHeaders(ctx),
    body: JSON.stringify(body),
  });

  setCliAnalyticsContext(ctx, {
    workspace_id: workspaceId,
    agent_id: res.agent_id,
    harness_config_version_id: res.config_version_id,
    run_snapshot_version: res.run_snapshot_version,
  });
  queueCliAnalyticsEvent(ctx, "harness_activated", {
    workspace_id: workspaceId,
    agent_id: res.agent_id,
    harness_config_version_id: res.config_version_id,
  });
  if (ctx.jsonMode) printJson(ctx.stdout, res);
  else {
    printSection(ctx.stdout, "Harness activated");
    printKeyValue(ctx.stdout, {
      workspace_id: workspaceId,
      agent_id: res.agent_id,
      config_version_id: res.config_version_id,
      run_snapshot_version: res.run_snapshot_version,
    });
  }
  return 0;
}
