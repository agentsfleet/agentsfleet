// M17_001 §3: zombiectl runs cancel <run_id>

function commandRuns(ctx, args, deps) {
  const { parseFlags, printJson, request, apiHeaders, ui, writeLine } = deps;

  async function cancel(subArgs) {
    const parsed = parseFlags(subArgs);
    const runId = parsed.positionals[0];
    if (!runId) {
      writeLine(ctx.stderr, ui.err("usage: zombiectl runs cancel <run_id>"));
      return 2;
    }

    const result = await request(ctx, `/v1/runs/${encodeURIComponent(runId)}:cancel`, {
      method: "POST",
      headers: apiHeaders(ctx),
    });

    if (ctx.jsonMode) {
      printJson(ctx.stdout, result);
    } else {
      writeLine(ctx.stdout, ui.ok(`Run ${runId} cancel requested`));
    }
    return 0;
  }

  const action = args[0];
  if (action === "cancel") return cancel(args.slice(1));

  writeLine(ctx.stderr, ui.err(`unknown runs subcommand: ${action}`));
  writeLine(ctx.stderr, ui.err("available: cancel"));
  return Promise.resolve(2);
}

export { commandRuns };
