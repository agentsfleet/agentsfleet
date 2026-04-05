// M17_001 §3: zombiectl runs cancel <run_id>
import { commandRunsInterrupt } from "./run_interrupt.js";

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

  async function replay(subArgs) {
    const parsed = parseFlags(subArgs);
    const runId = parsed.positionals[0];
    if (!runId) {
      writeLine(ctx.stderr, ui.err("usage: zombiectl runs replay <run_id>"));
      return 2;
    }

    const result = await request(ctx, `/v1/runs/${encodeURIComponent(runId)}:replay`, {
      method: "GET",
      headers: apiHeaders(ctx),
    });

    if (ctx.jsonMode) {
      printJson(ctx.stdout, result);
    } else {
      const gates = Array.isArray(result.gate_results) ? result.gate_results : [];
      if (gates.length === 0) {
        writeLine(ctx.stdout, ui.info("no gate results"));
      } else {
        for (const g of gates) {
          const outcome = g.exit_code === 0 ? "PASS" : "FAIL";
          writeLine(ctx.stdout, `[${g.gate_name}] ${outcome} (loop ${g.attempt ?? 0}, ${g.wall_ms ?? 0}ms)`);
          if (g.stdout_tail) writeLine(ctx.stdout, `  stdout: ${g.stdout_tail.slice(-200)}`);
          if (g.stderr_tail) writeLine(ctx.stdout, `  stderr: ${g.stderr_tail.slice(-200)}`);
        }
      }
    }
    return 0;
  }

  const action = args[0];
  if (action === "cancel") return cancel(args.slice(1));
  if (action === "replay") return replay(args.slice(1));
  if (action === "interrupt") return commandRunsInterrupt(ctx, args.slice(1), deps);

  writeLine(ctx.stderr, ui.err(`unknown runs subcommand: ${action}`));
  writeLine(ctx.stderr, ui.err("available: cancel, replay, interrupt"));
  return Promise.resolve(2);
}

export { commandRuns };
