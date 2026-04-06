import { setCliAnalyticsContext } from "../lib/analytics.js";
import { commandHarnessSourcePut } from "./harness_source.js";
import { commandHarnessCompile } from "./harness_compile.js";
import { commandHarnessActivate } from "./harness_activate.js";
import { commandHarnessActive } from "./harness_active.js";
import { validateRequiredId } from "../program/validate.js";
import { writeError } from "../program/io.js";

export async function commandHarness(ctx, args, workspaces, deps) {
  const { parseFlags, printJson, ui, writeLine } = deps;

  const group = args[0];
  const action = group === "source" ? args[1] : null;
  const parsed = parseFlags(group === "source" ? args.slice(2) : args.slice(1));

  const workspaceId = parsed.options["workspace-id"] || workspaces.current_workspace_id;
  if (!workspaceId) {
    writeError(ctx, "USAGE_ERROR", "workspace_id required", deps);
    return 2;
  }

  const wsCheck = validateRequiredId(workspaceId, "workspace_id");
  if (!wsCheck.ok) {
    writeError(ctx, "VALIDATION_ERROR", wsCheck.message, deps);
    return 2;
  }
  setCliAnalyticsContext(ctx, { workspace_id: workspaceId });

  if (group === "source" && action === "put") return commandHarnessSourcePut(ctx, parsed, workspaceId, deps);
  if (group === "compile") return commandHarnessCompile(ctx, parsed, workspaceId, deps);
  if (group === "activate") return commandHarnessActivate(ctx, parsed, workspaceId, deps);
  if (group === "active") return commandHarnessActive(ctx, parsed, workspaceId, deps);

  writeError(ctx, "UNKNOWN_COMMAND", "usage: harness source put|compile|activate|active", deps);
  return 2;
}
