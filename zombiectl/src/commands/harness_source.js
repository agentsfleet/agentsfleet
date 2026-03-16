import fs from "node:fs/promises";
import path from "node:path";

export async function commandHarnessSourcePut(ctx, parsed, workspaceId, deps) {
  const {
    request,
    apiHeaders,
    ui,
    printJson,
    writeLine,
    readFile = fs.readFile,
    resolvePath = path.resolve,
  } = deps;

  const file = parsed.options.file;
  if (!file) {
    writeLine(ctx.stderr, ui.err("harness source put requires --file"));
    return 2;
  }

  const fileContent = await readFile(resolvePath(file), "utf8");

  const MAX_SIZE = 2 * 1024 * 1024;
  const sizeBytes = Buffer.byteLength(fileContent, "utf8");
  if (sizeBytes > MAX_SIZE) {
    writeLine(ctx.stderr, ui.err(`file too large: ${sizeBytes} bytes (max 2MB)`));
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

  if (ctx.jsonMode) printJson(ctx.stdout, res);
  else writeLine(ctx.stdout, ui.ok(`harness source stored config_version_id=${res.config_version_id}`));
  return 0;
}
