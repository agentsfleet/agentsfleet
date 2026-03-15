export async function commandAgent(ctx, args, workspaces, deps) {
  const { parseFlags, request, apiHeaders, ui, printJson, printTable, printKeyValue, writeLine } = deps;

  const action = args[0];
  const tail = args.slice(1);

  if (action === "scores") {
    const parsed = parseFlags(tail);
    const agentId = parsed.positionals[0] || parsed.options["agent-id"];
    if (!agentId) {
      writeLine(ctx.stderr, ui.err("agent scores requires <agent-id>"));
      return 2;
    }

    const limit = parsed.options.limit || 20;
    const cursor = parsed.options.cursor || null;

    let url = `/v1/agents/${encodeURIComponent(agentId)}/scores?limit=${limit}`;
    if (cursor) url += `&cursor=${encodeURIComponent(cursor)}`;

    const res = await request(ctx, url, { method: "GET", headers: apiHeaders(ctx) });

    if (ctx.jsonMode) {
      printJson(ctx.stdout, res);
    } else {
      const items = Array.isArray(res.items) ? res.items : [];
      if (items.length === 0) {
        writeLine(ctx.stdout, ui.info("no scores"));
      } else {
        printTable(ctx.stdout, [
          { key: "score_id", label: "SCORE_ID" },
          { key: "run_id", label: "RUN_ID" },
          { key: "score", label: "SCORE" },
          { key: "scored_at", label: "SCORED_AT" },
        ], items);
      }
      if (res.next_cursor != null) {
        writeLine(ctx.stdout, ui.dim(`next: --cursor ${res.next_cursor}`));
      }
    }
    return 0;
  }

  if (action === "profile") {
    const parsed = parseFlags(tail);
    const agentId = parsed.positionals[0] || parsed.options["agent-id"];
    if (!agentId) {
      writeLine(ctx.stderr, ui.err("agent profile requires <agent-id>"));
      return 2;
    }

    const res = await request(ctx, `/v1/agents/${encodeURIComponent(agentId)}`, {
      method: "GET",
      headers: apiHeaders(ctx),
    });

    if (ctx.jsonMode) {
      printJson(ctx.stdout, res);
    } else {
      printKeyValue(ctx.stdout, {
        agent_id: res.agent_id,
        name: res.name,
        status: res.status,
        workspace_id: res.workspace_id,
        created_at: res.created_at,
        updated_at: res.updated_at,
      });
    }
    return 0;
  }

  writeLine(ctx.stderr, ui.err("usage: agent scores <agent-id> [--limit N] [--cursor T] [--json]"));
  writeLine(ctx.stderr, ui.err("       agent profile <agent-id> [--json]"));
  return 2;
}
