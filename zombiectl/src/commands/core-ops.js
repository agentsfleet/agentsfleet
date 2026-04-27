import { wsZombiesPath } from "../lib/api-paths.js";

const PER_CHECK_TIMEOUT_MS = 5000;

function createCoreOpsHandlers(ctx, workspaces, deps) {
  const {
    apiHeaders,
    printJson,
    request,
    ui,
    writeLine,
  } = deps;

  async function commandDoctor() {
    const checks = [];

    // 1. server_reachable — GET /healthz returns {status: "ok"} within 5s.
    try {
      const healthz = await request(ctx, "/healthz", {
        method: "GET",
        timeoutMs: PER_CHECK_TIMEOUT_MS,
      });
      const ok = healthz?.status === "ok";
      checks.push({
        name: "server_reachable",
        ok,
        detail: ok ? `${ctx.apiUrl}/healthz` : `unexpected payload: ${JSON.stringify(healthz)}`,
      });
    } catch (err) {
      checks.push({
        name: "server_reachable",
        ok: false,
        detail: `${ctx.apiUrl}/healthz: ${err?.message ?? String(err)}`,
      });
    }

    // 2. workspace_selected — local config has a current_workspace_id.
    const wsId = workspaces.current_workspace_id;
    const wsSelected = Boolean(wsId);
    checks.push({
      name: "workspace_selected",
      ok: wsSelected,
      detail: wsSelected ? wsId : "no workspace selected. Run: zombiectl workspace add",
    });

    // 3. workspace_binding_valid — token is bound to the selected workspace.
    //    Probe via GET /v1/workspaces/{ws}/zombies (canonical workspace-scoped
    //    read; returns 200 with an empty list if no zombies). Skips when
    //    workspace_selected already failed — no point hitting the server with
    //    an empty workspace id.
    if (!wsSelected) {
      checks.push({
        name: "workspace_binding_valid",
        ok: false,
        detail: "skipped: no workspace selected",
      });
    } else {
      try {
        await request(ctx, wsZombiesPath(wsId), {
          method: "GET",
          headers: apiHeaders ? apiHeaders(ctx) : {},
          timeoutMs: PER_CHECK_TIMEOUT_MS,
        });
        checks.push({
          name: "workspace_binding_valid",
          ok: true,
          detail: `token bound to ${wsId}`,
        });
      } catch (err) {
        const code = err?.code || "REQUEST_FAILED";
        checks.push({
          name: "workspace_binding_valid",
          ok: false,
          detail: `${wsId}: ${code} — run \`zombiectl workspace list\` to reset`,
        });
      }
    }

    const ok = checks.every((c) => c.ok);
    const report = { ok, api_url: ctx.apiUrl, checks };

    if (ctx.jsonMode) {
      printJson(ctx.stdout, report);
    } else {
      writeLine(ctx.stdout, ui.head("zombiectl doctor"));
      writeLine(ctx.stdout);
      for (const c of checks) {
        const tag = c.ok ? "[OK]" : "[FAIL]";
        const line = `${tag} ${c.name}`;
        writeLine(ctx.stdout, c.ok ? ui.ok(line) : ui.err(line));
        if (!c.ok && c.detail) writeLine(ctx.stdout, `        ${c.detail}`);
      }
      writeLine(ctx.stdout);
      const passed = checks.filter((c) => c.ok).length;
      writeLine(
        ctx.stdout,
        ok ? ui.ok("All checks passed.") : ui.err(`${passed}/${checks.length} checks passed`),
      );
    }
    return ok ? 0 : 1;
  }

  return {
    commandDoctor,
  };
}

export { createCoreOpsHandlers };
