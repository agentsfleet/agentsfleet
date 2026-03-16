export async function commandHarnessCompile(ctx, parsed, workspaceId, deps) {
  const { request, apiHeaders, ui, printJson, writeLine } = deps;

  const body = {
    agent_id: parsed.options["agent-id"] || null,
    config_version_id: parsed.options["config-version-id"] || null,
  };

  const res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/harness/compile`, {
    method: "POST",
    headers: apiHeaders(ctx),
    body: JSON.stringify(body),
  });

  if (ctx.jsonMode) printJson(ctx.stdout, res);
  else writeLine(ctx.stdout, ui.ok(`compile_job_id=${res.compile_job_id} valid=${res.is_valid}`));
  return 0;
}
