import { wsZombiesPath, HEALTHZ_PATH, HEALTHZ_STATUS_OK } from "../lib/api-paths.js";
import { AUTH_PRESET, compose } from "../lib/error-map-presets.js";
import {
  ERR_INTERNAL_DB_UNAVAILABLE,
  ERR_INTERNAL_DB_QUERY,
  ERR_INTERNAL_OPERATION_FAILED,
} from "../constants/error-codes.ts";
import { DOCTOR_CHECK } from "../constants/doctor-checks.ts";

const PER_CHECK_TIMEOUT_MS = 5000;

// Doctor hits /healthz (no auth) and the workspace zombies list (auth).
// UZ-INTERNAL-001 (DB unavailable) is the headline failure surfaced by
// failure-modes.integration.test.js. AUTH_PRESET covers the
// authenticated leg.
export const doctorErrorMap = compose(AUTH_PRESET, {
  [ERR_INTERNAL_DB_UNAVAILABLE]: {
    code: "SERVER_INTERNAL",
    message: "Database unavailable — the API is degraded; try again shortly.",
  },
  [ERR_INTERNAL_DB_QUERY]: {
    code: "SERVER_INTERNAL",
    message: "Server internal error — the API is degraded; try again shortly.",
  },
  [ERR_INTERNAL_OPERATION_FAILED]: {
    code: "SERVER_INTERNAL",
    message: "Server internal error — the API is degraded; try again shortly.",
  },
});

export async function commandDoctor(ctx, _parsed, workspaces, deps) {
  const {
    apiHeaders,
    printJson,
    printSection,
    request,
    ui,
    writeLine,
  } = deps;

  const checks = [];

  try {
    const healthz = await request(ctx, HEALTHZ_PATH, {
      method: "GET",
      timeoutMs: PER_CHECK_TIMEOUT_MS,
    });
    const ok = healthz?.status === HEALTHZ_STATUS_OK;
    checks.push({
      name: DOCTOR_CHECK.SERVER_REACHABLE,
      ok,
      detail: ok ? `${ctx.apiUrl}${HEALTHZ_PATH}` : `unexpected payload: ${JSON.stringify(healthz)}`,
    });
  } catch (err) {
    checks.push({
      name: DOCTOR_CHECK.SERVER_REACHABLE,
      ok: false,
      detail: `${ctx.apiUrl}${HEALTHZ_PATH}: ${err?.message ?? String(err)}`,
    });
  }

  const wsId = workspaces.current_workspace_id;
  const wsSelected = Boolean(wsId);
  checks.push({
    name: DOCTOR_CHECK.WORKSPACE_SELECTED,
    ok: wsSelected,
    detail: wsSelected ? wsId : "no workspace selected. Run: zombiectl workspace add",
  });

  if (!wsSelected) {
    checks.push({
      name: DOCTOR_CHECK.WORKSPACE_BINDING_VALID,
      ok: false,
      detail: "skipped: no workspace selected",
    });
  } else {
    try {
      await request(ctx, wsZombiesPath(wsId), {
        method: "GET",
        headers: apiHeaders(ctx),
        timeoutMs: PER_CHECK_TIMEOUT_MS,
      });
      checks.push({
        name: DOCTOR_CHECK.WORKSPACE_BINDING_VALID,
        ok: true,
        detail: `token bound to ${wsId}`,
      });
    } catch (err) {
      const code = err?.code || "REQUEST_FAILED";
      checks.push({
        name: DOCTOR_CHECK.WORKSPACE_BINDING_VALID,
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
    printSection(ctx.stdout, "zombiectl doctor");
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
