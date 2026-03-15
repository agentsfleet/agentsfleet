export async function commandHarnessActive(ctx, parsed, workspaceId, deps) {
  const { request, apiHeaders, ui, printJson, writeLine } = deps;

  const res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/harness/active`, {
    method: "GET",
    headers: apiHeaders(ctx),
  });

  if (ctx.jsonMode) printJson(ctx.stdout, res);
  else writeLine(
    ctx.stdout,
    ui.info(
      `active agent_id=${res.agent_id ?? "default-v1"} config_version_id=${res.config_version_id ?? "default-v1"} run_snapshot_version=${res.run_snapshot_version ?? "default-v1"}`,
    ),
  );
  return 0;
}
