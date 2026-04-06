import fs from "node:fs/promises";
import path from "node:path";
import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";
import { writeError } from "../program/io.js";

export async function commandHarnessSourcePut(ctx, parsed, workspaceId, deps) {
  const {
    request,
    apiHeaders,
    ui,
    printJson,
    printKeyValue = () => {},
    printSection = () => {},
    writeLine,
    readFile = fs.readFile,
    resolvePath = path.resolve,
  } = deps;

  const file = parsed.options.file;
  if (!file) {
    writeError(ctx, "USAGE_ERROR", "harness source put requires --file", deps);
    return 2;
  }

  const fileContent = await readFile(resolvePath(file), "utf8");

  const MAX_SIZE = 2 * 1024 * 1024;
  const sizeBytes = Buffer.byteLength(fileContent, "utf8");
  if (sizeBytes > MAX_SIZE) {
    writeError(ctx, "VALIDATION_ERROR", `file too large: ${sizeBytes} bytes (max 2MB)`, deps);
    return 2;
  }

  if (!ctx.jsonMode) {
    writeLine(ctx.stdout, ui.info(`uploading ${path.basename(String(file))} (${sizeBytes} bytes)`));
  }

  const inferredName = path.basename(String(file), path.extname(String(file)));
  const body = {
    agent_id: parsed.options["agent-id"] || null,
    name: parsed.options.name || inferredName || "Workspace Harness",
    source_markdown: fileContent,
  };

  const res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/harness/source`, {
    method: "PUT",
    headers: apiHeaders(ctx),
    body: JSON.stringify(body),
  });

  setCliAnalyticsContext(ctx, {
    workspace_id: workspaceId,
    agent_id: body.agent_id,
    harness_name: body.name,
    harness_config_version_id: res.config_version_id,
    harness_source_bytes: sizeBytes,
  });
  queueCliAnalyticsEvent(ctx, "harness_source_uploaded", {
    workspace_id: workspaceId,
    harness_config_version_id: res.config_version_id,
  });
  if (ctx.jsonMode) printJson(ctx.stdout, res);
  else {
    printSection(ctx.stdout, "Harness source stored");
    printKeyValue(ctx.stdout, {
      workspace_id: workspaceId,
      config_version_id: res.config_version_id,
      name: body.name,
      size_bytes: sizeBytes,
    });
  }
  return 0;
}
