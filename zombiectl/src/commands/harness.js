import fs from "node:fs/promises";
import path from "node:path";

export async function commandHarness(ctx, args, workspaces, deps) {
  const {
    parseFlags,
    request,
    apiHeaders,
    ui,
    printJson,
    writeLine,
    readFile = fs.readFile,
    resolvePath = path.resolve,
  } = deps;

  const group = args[0];
  const action = group === "source" ? args[1] : null;
  const parsed = parseFlags(group === "source" ? args.slice(2) : args.slice(1));

  const workspaceId = parsed.options["workspace-id"] || workspaces.current_workspace_id;
  if (!workspaceId) {
    writeLine(ctx.stderr, "workspace_id required");
    return 2;
  }

  if (group === "source" && action === "put") {
    const file = parsed.options.file;
    if (!file) {
      writeLine(ctx.stderr, ui.err("harness source put requires --file"));
      return 2;
    }
    const fileContent = await readFile(resolvePath(file), "utf8");
    const inferredName = path.basename(String(file), path.extname(String(file)));
    const body = {
      profile_id: parsed.options["profile-id"] || null,
      name: parsed.options.name || inferredName || "Workspace Harness",
      source_markdown: fileContent,
    };
    const res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/harness/source`, {
      method: "PUT",
      headers: apiHeaders(ctx),
      body: JSON.stringify(body),
    });
    if (ctx.jsonMode) printJson(ctx.stdout, res);
    else writeLine(ctx.stdout, ui.ok(`harness source stored profile_version_id=${res.profile_version_id}`));
    return 0;
  }

  if (group === "compile" && action === null) {
    const profileId = parsed.options["profile-id"] || null;
    const profileVersionId = parsed.options["profile-version-id"] || null;
    const body = {
      profile_id: profileId,
      profile_version_id: profileVersionId,
    };
    const res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/harness/compile`, {
      method: "POST",
      headers: apiHeaders(ctx),
      body: JSON.stringify(body),
    });
    if (ctx.jsonMode) printJson(ctx.stdout, res);
    else writeLine(ctx.stdout, `compile_job_id=${res.compile_job_id} valid=${res.is_valid}`);
    return 0;
  }

  if (group === "activate" && action === null) {
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
    else writeLine(ctx.stdout, ui.ok(`activated profile_version_id=${res.profile_version_id}`));
    return 0;
  }

  if (group === "active" && action === null) {
    const res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/harness/active`, {
      method: "GET",
      headers: apiHeaders(ctx),
    });
    if (ctx.jsonMode) printJson(ctx.stdout, res);
    else writeLine(ctx.stdout, ui.info(`active profile_version_id=${res.profile_version_id}`));
    return 0;
  }

  writeLine(ctx.stderr, ui.err("usage: harness source put|compile|activate|active"));
  return 2;
}
