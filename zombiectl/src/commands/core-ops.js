function createCoreOpsHandlers(ctx, workspaces, deps) {
  const {
    apiHeaders,
    parseFlags,
    printJson,
    request,
    ui,
    writeLine,
  } = deps;

  async function commandDoctor() {
    const checks = [];

    try {
      const healthz = await request(ctx, "/healthz", { method: "GET" });
      checks.push({ name: "healthz", ok: healthz.status === "ok", detail: healthz });
    } catch (err) {
      checks.push({ name: "healthz", ok: false, detail: String(err) });
    }

    try {
      const readyz = await request(ctx, "/readyz", { method: "GET" });
      checks.push({ name: "readyz", ok: readyz.ready === true, detail: readyz });
    } catch (err) {
      checks.push({ name: "readyz", ok: false, detail: String(err) });
    }

    checks.push({ name: "credentials", ok: Boolean(ctx.token || ctx.apiKey), detail: ctx.token ? "token" : ctx.apiKey ? "api_key" : "missing" });
    checks.push({ name: "workspace", ok: Boolean(workspaces.current_workspace_id), detail: workspaces.current_workspace_id || "missing" });

    const ok = checks.every((c) => c.ok);
    const report = { ok, api_url: ctx.apiUrl, checks };

    if (ctx.jsonMode) {
      printJson(ctx.stdout, report);
    } else {
      writeLine(ctx.stdout, ui.head("doctor"));
      for (const c of checks) writeLine(ctx.stdout, `${c.ok ? ui.ok(c.name) : ui.err(c.name)}`);
    }
    return ok ? 0 : 1;
  }

  async function commandSkillSecret(args) {
    const action = args[0];
    const parsed = parseFlags(args.slice(1));
    const workspaceId = parsed.options["workspace-id"] || workspaces.current_workspace_id;
    const skillRef = parsed.options["skill-ref"];
    const key = parsed.options.key;

    if (!workspaceId || !skillRef || !key) {
      writeLine(ctx.stderr, ui.err("skill-secret requires --workspace-id --skill-ref --key"));
      return 2;
    }

    const route = `/v1/workspaces/${encodeURIComponent(workspaceId)}/skills/${encodeURIComponent(skillRef)}/secrets/${encodeURIComponent(key)}`;

    if (action === "put") {
      if (!parsed.options.value) {
        writeLine(ctx.stderr, ui.err("skill-secret put requires --value"));
        return 2;
      }
      const body = {
        value: String(parsed.options.value),
        scope: parsed.options.scope || "sandbox",
        meta: {},
      };
      const res = await request(ctx, route, {
        method: "PUT",
        headers: apiHeaders(ctx),
        body: JSON.stringify(body),
      });
      if (ctx.jsonMode) printJson(ctx.stdout, res);
      else writeLine(ctx.stdout, ui.ok("skill secret stored"));
      return 0;
    }

    if (action === "delete") {
      const res = await request(ctx, route, {
        method: "DELETE",
        headers: apiHeaders(ctx),
      });
      if (ctx.jsonMode) printJson(ctx.stdout, res);
      else writeLine(ctx.stdout, ui.ok("skill secret deleted"));
      return 0;
    }

    writeLine(ctx.stderr, ui.err("usage: skill-secret put|delete ..."));
    return 2;
  }

  return {
    commandDoctor,
    commandSkillSecret,
  };
}

export { createCoreOpsHandlers };
