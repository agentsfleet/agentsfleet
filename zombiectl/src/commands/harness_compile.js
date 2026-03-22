import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";

export async function commandHarnessCompile(ctx, parsed, workspaceId, deps) {
  const { request, apiHeaders, printJson, printKeyValue = () => {}, printSection = () => {} } = deps;

  const body = {
    agent_id: parsed.options["agent-id"] || null,
    config_version_id: parsed.options["config-version-id"] || null,
  };

  const res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/harness/compile`, {
    method: "POST",
    headers: apiHeaders(ctx),
    body: JSON.stringify(body),
  });

  setCliAnalyticsContext(ctx, {
    workspace_id: workspaceId,
    agent_id: body.agent_id,
    harness_config_version_id: body.config_version_id,
    compile_job_id: res.compile_job_id,
    harness_valid: res.is_valid,
  });
  queueCliAnalyticsEvent(ctx, "harness_compiled", {
    workspace_id: workspaceId,
    compile_job_id: res.compile_job_id,
    harness_valid: res.is_valid,
  });
  if (ctx.jsonMode) printJson(ctx.stdout, res);
  else {
    printSection(ctx.stdout, "Harness compile");
    printKeyValue(ctx.stdout, {
      workspace_id: workspaceId,
      compile_job_id: res.compile_job_id,
      valid: res.is_valid,
      config_version_id: body.config_version_id ?? "latest",
    });
  }
  return 0;
}
