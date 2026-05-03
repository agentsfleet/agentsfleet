import { writeError } from "../program/io.js";

export async function commandAdmin(ctx, args, workspaces, deps) {
  const { parseFlags, ui, writeLine } = deps;
  const group = args[0];
  const action = args[1];
  const key = args[2];
  const parsed = parseFlags(args.slice(3));

  if (group === "config" && action === "add" && key === "scoring_context_max_tokens") {
    const workspaceId = parsed.options["workspace-id"];
    const rawValue = parsed.positionals[0] || parsed.options.value;
    if (!workspaceId) {
      writeError(ctx, "USAGE_ERROR", "admin config add scoring_context_max_tokens requires --workspace-id", deps);
      return 2;
    }
    if (!rawValue) {
      writeError(ctx, "USAGE_ERROR", "admin config add scoring_context_max_tokens requires <value>", deps);
      return 2;
    }
    const parsedValue = Number.parseInt(rawValue, 10);
    if (!Number.isInteger(parsedValue) || parsedValue < 512 || parsedValue > 8192) {
      writeError(ctx, "VALIDATION_ERROR", "scoring_context_max_tokens must be an integer between 512 and 8192", deps);
      return 2;
    }
    const res = await deps.request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}/scoring/config`, {
      method: "POST",
      headers: deps.apiHeaders(ctx),
      body: JSON.stringify({ scoring_context_max_tokens: parsedValue }),
    });
    if (ctx.jsonMode) {
      deps.printJson(ctx.stdout, res);
    } else {
      writeLine(ctx.stdout, ui.ok(`workspace scoring_context_max_tokens=${res.scoring_context_max_tokens}`));
    }
    return 0;
  }

  writeError(ctx, "UNKNOWN_COMMAND", `unknown admin command: ${group ?? "(none)"}`, deps);
  return 2;
}
