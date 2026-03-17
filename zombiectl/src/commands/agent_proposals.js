import { AGENTS_PATH } from "../lib/api-paths.js";

export async function commandAgentProposals(ctx, parsed, agentId, deps) {
  const { request, apiHeaders, printJson, printTable, ui, writeLine } = deps;

  const subaction = parsed.positionals[1] || "list";
  const proposalId = parsed.positionals[2] || null;

  if (subaction === "list") {
    const res = await request(ctx, `${AGENTS_PATH}${encodeURIComponent(agentId)}/proposals`, {
      method: "GET",
      headers: apiHeaders(ctx),
    });

    if (ctx.jsonMode) {
      printJson(ctx.stdout, res);
    } else {
      const items = Array.isArray(res.data) ? res.data : [];
      if (items.length === 0) {
        writeLine(ctx.stdout, ui.info("no pending manual proposals"));
      } else {
        printTable(ctx.stdout, [
          { key: "proposal_id", label: "PROPOSAL_ID" },
          { key: "trigger_reason", label: "TRIGGER" },
          { key: "config_version_id", label: "CONFIG_VERSION_ID" },
          { key: "created_at", label: "CREATED_AT" },
        ], items);
      }
    }
    return 0;
  }

  if (!proposalId) {
    writeLine(ctx.stderr, ui.err(`agent proposals ${subaction} requires <proposal-id>`));
    return 2;
  }

  if (subaction === "approve") {
    const res = await request(
      ctx,
      `${AGENTS_PATH}${encodeURIComponent(agentId)}/proposals/${encodeURIComponent(proposalId)}:approve`,
      {
        method: "POST",
        headers: apiHeaders(ctx),
      },
    );
    if (ctx.jsonMode) {
      printJson(ctx.stdout, res);
    } else {
      writeLine(ctx.stdout, ui.ok(`approved ${res.proposal_id} -> ${res.status}`));
    }
    return 0;
  }

  if (subaction === "reject") {
    const reason = parsed.options.reason || null;
    const res = await request(
      ctx,
      `${AGENTS_PATH}${encodeURIComponent(agentId)}/proposals/${encodeURIComponent(proposalId)}:reject`,
      {
        method: "POST",
        headers: apiHeaders(ctx),
        body: JSON.stringify(reason ? { reason } : {}),
      },
    );
    if (ctx.jsonMode) {
      printJson(ctx.stdout, res);
    } else {
      writeLine(ctx.stdout, ui.ok(`rejected ${res.proposal_id} (${res.rejection_reason})`));
    }
    return 0;
  }

  writeLine(ctx.stderr, ui.err("usage: agent proposals <agent-id>"));
  writeLine(ctx.stderr, ui.err("       agent proposals <agent-id> approve <proposal-id>"));
  writeLine(ctx.stderr, ui.err("       agent proposals <agent-id> reject <proposal-id> [--reason TEXT]"));
  return 2;
}
