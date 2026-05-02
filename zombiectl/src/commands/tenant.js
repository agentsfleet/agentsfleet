// `zombiectl tenant <subgroup> <action>` parent dispatcher.
//
// Subgroups:
//   provider — get | set | reset (tenant_provider.js)

import {
  commandTenantProviderGet,
  commandTenantProviderSet,
  commandTenantProviderReset,
} from "./tenant_provider.js";
import { writeError } from "../program/io.js";

export async function commandTenant(ctx, args, _workspaces, deps) {
  const { parseFlags, ui, writeLine } = deps;

  const subgroup = args[0];
  const action = args[1];
  const parsed = parseFlags(args.slice(2));

  if (subgroup === "provider") {
    if (action === "get")   return commandTenantProviderGet(ctx, parsed, deps);
    if (action === "set")   return commandTenantProviderSet(ctx, parsed, deps);
    if (action === "reset") return commandTenantProviderReset(ctx, parsed, deps);

    if (ctx.jsonMode) {
      writeError(ctx, "UNKNOWN_COMMAND", `unknown tenant provider action: ${action ?? "(none)"}`, deps);
    } else {
      writeLine(ctx.stderr, ui.err("usage: zombiectl tenant provider get"));
      writeLine(ctx.stderr, ui.err("       zombiectl tenant provider set --credential <name> [--model <override>]"));
      writeLine(ctx.stderr, ui.err("       zombiectl tenant provider reset"));
    }
    return 2;
  }

  if (ctx.jsonMode) {
    writeError(ctx, "UNKNOWN_COMMAND", `unknown tenant subgroup: ${subgroup ?? "(none)"}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err("usage: zombiectl tenant provider {get|set|reset}"));
  }
  return 2;
}
