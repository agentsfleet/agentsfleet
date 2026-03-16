import { commandHarnessSourcePut } from "./harness_source.js";
import { commandHarnessCompile } from "./harness_compile.js";
import { commandHarnessActivate } from "./harness_activate.js";
import { commandHarnessActive } from "./harness_active.js";
import { validateRequiredId } from "../program/validate.js";

export async function commandHarness(ctx, args, workspaces, deps) {
  const { parseFlags, ui, writeLine } = deps;

  const group = args[0];
  const action = group === "source" ? args[1] : null;
  const parsed = parseFlags(group === "source" ? args.slice(2) : args.slice(1));

  const workspaceId = parsed.options["workspace-id"] || workspaces.current_workspace_id;
  if (!workspaceId) {
    writeLine(ctx.stderr, ui.err("workspace_id required"));
    return 2;
  }

  const wsCheck = validateRequiredId(workspaceId, "workspace_id");
  if (!wsCheck.ok) {
    writeLine(ctx.stderr, ui.err(wsCheck.message));
    return 2;
  }

  if (group === "source" && action === "put") return commandHarnessSourcePut(ctx, parsed, workspaceId, deps);
  if (group === "compile") return commandHarnessCompile(ctx, parsed, workspaceId, deps);
  if (group === "activate") return commandHarnessActivate(ctx, parsed, workspaceId, deps);
  if (group === "active") return commandHarnessActive(ctx, parsed, workspaceId, deps);

  writeLine(ctx.stderr, ui.err("usage: harness source put|compile|activate|active"));
  return 2;
}
