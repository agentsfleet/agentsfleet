// `zombiectl tenant <subgroup> <action>` parent dispatcher.
//
// Subgroups:
//   provider — show | add | delete (tenant_provider.js)

import {
  commandTenantProviderShow,
  commandTenantProviderAdd,
  commandTenantProviderDelete,
} from "./tenant_provider.js";
import { writeError } from "../program/io.js";

export async function commandTenant(ctx, args, _workspaces, deps) {
  const { parseFlags, ui, writeLine } = deps;

  const subgroup = args[0];
  const action = args[1];
  const parsed = parseFlags(args.slice(2));

  if (subgroup === "provider") {
    if (action === "show")   return commandTenantProviderShow(ctx, parsed, deps);
    if (action === "add")    return commandTenantProviderAdd(ctx, parsed, deps);
    if (action === "delete") return commandTenantProviderDelete(ctx, parsed, deps);

    if (ctx.jsonMode) {
      writeError(ctx, "UNKNOWN_COMMAND", `unknown tenant provider action: ${action ?? "(none)"}`, deps);
    } else {
      writeLine(ctx.stderr, ui.err("usage: zombiectl tenant provider show"));
      writeLine(ctx.stderr, ui.err("       zombiectl tenant provider add --credential <name> [--model <override>]"));
      writeLine(ctx.stderr, ui.err("       zombiectl tenant provider delete"));
    }
    return 2;
  }

  if (ctx.jsonMode) {
    writeError(ctx, "UNKNOWN_COMMAND", `unknown tenant subgroup: ${subgroup ?? "(none)"}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err("usage: zombiectl tenant provider {show|add|delete}"));
  }
  return 2;
}
