export async function commandHarnessActivate(ctx, parsed, workspaceId, deps) {
  const { request, apiHeaders, ui, printJson, writeLine } = deps;

  const profileVersionId = parsed.options["profile-version-id"];
  if (!profileVersionId) {
    writeLine(ctx.stderr, ui.err("harness activate requires --profile-version-id"));
    return 2;
  }

  const body = {
    profile_version_id: profileVersionId,
    activated_by: parsed.options["activated-by"] || "zombiectl",
  };

  const res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/harness/activate`, {
    method: "POST",
    headers: apiHeaders(ctx),
    body: JSON.stringify(body),
  });

  if (ctx.jsonMode) printJson(ctx.stdout, res);
  else writeLine(
    ctx.stdout,
    ui.ok(
      `activated profile_id=${res.profile_id} profile_version_id=${res.profile_version_id} run_snapshot_version=${res.run_snapshot_version}`,
    ),
  );
  return 0;
}
