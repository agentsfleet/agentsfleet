// M21_001 §3: zombiectl runs interrupt <run_id> <message> [--mode=queued|instant]
//
// Sends an interrupt to a running agent. The agent absorbs the message
// and adjusts course — it does NOT abort or restart.
//
// DX contract:
//   - Returns immediately after the server acks (does not block on delivery)
//   - Prints the effective mode (queued/instant) and ack status
//   - Exit 0 on success, 1 on error

const MAX_MESSAGE_BYTES = 4096;

function commandRunsInterrupt(ctx, args, deps) {
  const { parseFlags, printJson, request, apiHeaders, ui, writeLine } = deps;

  const parsed = parseFlags(args);
  const runId = parsed.positionals[0];
  const messageParts = parsed.positionals.slice(1);

  if (!runId || messageParts.length === 0) {
    writeLine(ctx.stderr, ui.err("usage: zombiectl runs interrupt <run_id> <message> [--mode=queued|instant]"));
    writeLine(ctx.stderr, ui.dim("  Sends a steering message to a running agent without stopping it."));
    writeLine(ctx.stderr, ui.dim("  --mode=queued   (default) delivered at next gate checkpoint"));
    writeLine(ctx.stderr, ui.dim("  --mode=instant  delivered mid-turn if executor is active"));
    return Promise.resolve(2);
  }

  const message = messageParts.join(" ");
  if (message.length > MAX_MESSAGE_BYTES) {
    writeLine(ctx.stderr, ui.err(`Message too long (${message.length} bytes, max ${MAX_MESSAGE_BYTES})`));
    return Promise.resolve(2);
  }

  const mode = parsed.flags.mode || "queued";
  if (mode !== "queued" && mode !== "instant") {
    writeLine(ctx.stderr, ui.err("--mode must be 'queued' or 'instant'"));
    return Promise.resolve(2);
  }

  return (async () => {
    const result = await request(ctx, `/v1/runs/${encodeURIComponent(runId)}:interrupt`, {
      method: "POST",
      headers: { ...apiHeaders(ctx), "Content-Type": "application/json" },
      body: JSON.stringify({ message, mode }),
    });

    if (ctx.jsonMode) {
      printJson(ctx.stdout, result);
    } else {
      writeLine(ctx.stdout, ui.ok(`Interrupt sent (mode: ${result.mode || mode})`));
    }
    return 0;
  })();
}

export { commandRunsInterrupt, MAX_MESSAGE_BYTES };
