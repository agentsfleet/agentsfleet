function createCoreOpsHandlers(ctx, workspaces, deps) {
  const {
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
      writeLine(ctx.stdout, ui.head("zombiectl doctor"));
      writeLine(ctx.stdout);
      for (const c of checks) {
        const tag = c.ok ? "[OK]" : "[FAIL]";
        writeLine(ctx.stdout, c.ok ? ui.ok(`${tag} ${c.name}`) : ui.err(`${tag} ${c.name}`));
      }
      writeLine(ctx.stdout);
      const passed = checks.filter((c) => c.ok).length;
      if (ok) {
        writeLine(ctx.stdout, ui.ok(`All checks passed.`));
      } else {
        writeLine(ctx.stdout, ui.err(`${passed}/${checks.length} checks passed`));
      }
    }
    return ok ? 0 : 1;
  }

  return {
    commandDoctor,
  };
}

export { createCoreOpsHandlers };
