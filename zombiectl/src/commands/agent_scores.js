export async function commandAgentScores(ctx, parsed, agentId, deps) {
  const { request, apiHeaders, ui, printJson, printTable, writeLine } = deps;

  const limit = parsed.options.limit || 20;
  const startingAfter = parsed.options["starting-after"] || null;

  let url = `/v1/agents/${encodeURIComponent(agentId)}/scores?limit=${limit}`;
  if (startingAfter) url += `&starting_after=${encodeURIComponent(startingAfter)}`;

  const res = await request(ctx, url, { method: "GET", headers: apiHeaders(ctx) });

  if (ctx.jsonMode) {
    printJson(ctx.stdout, res);
  } else {
    const items = Array.isArray(res.data) ? res.data : [];
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
    if (res.has_more && res.next_cursor) {
      writeLine(ctx.stdout, ui.dim(`next: --starting-after ${res.next_cursor}`));
    }
  }
  return 0;
}
