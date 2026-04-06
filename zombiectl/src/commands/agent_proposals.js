import { AGENTS_PATH } from "../lib/api-paths.js";
import { queueCliAnalyticsEvent, setCliAnalyticsContext } from "../lib/analytics.js";
import { writeError } from "../program/io.js";

export async function commandAgentProposals(ctx, parsed, agentId, deps) {
  const { request, apiHeaders, printJson, printSection = () => {}, printTable, ui, writeLine } = deps;

  const subaction = parsed.positionals[1] || "list";
  const proposalId = parsed.positionals[2] || null;

  if (subaction === "list") {
    const res = await request(ctx, `${AGENTS_PATH}${encodeURIComponent(agentId)}/proposals`, {
      method: "GET",
      headers: apiHeaders(ctx),
    });
    const items = Array.isArray(res.data) ? res.data : [];
    setCliAnalyticsContext(ctx, {
      agent_id: agentId,
      proposal_count: items.length,
    });
    queueCliAnalyticsEvent(ctx, "agent_proposals_viewed", {
      agent_id: agentId,
      proposal_count: items.length,
    });

    if (ctx.jsonMode) {
      printJson(ctx.stdout, res);
    } else {
      if (items.length === 0) {
        writeLine(ctx.stdout, ui.info("no open proposals"));
      } else {
        printSection(ctx.stdout, `Agent proposals · ${agentId}`);
        printTable(ctx.stdout, [
          { key: "proposal_id", label: "PROPOSAL_ID" },
          { key: "status", label: "STATUS" },
          { key: "trigger_reason", label: "TRIGGER" },
          { key: "action", label: "ACTION" },
          { key: "config_version_id", label: "CONFIG_VERSION_ID" },
          { key: "created_at", label: "CREATED_AT" },
        ], items.map((item) => ({
          ...item,
          action: describeProposalAction(item),
        })));
      }
    }
    return 0;
  }

  if (!proposalId) {
    writeError(ctx, "USAGE_ERROR", `agent proposals ${subaction} requires <proposal-id>`, deps);
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
    setCliAnalyticsContext(ctx, {
      agent_id: agentId,
      proposal_id: res.proposal_id,
      proposal_status: res.status,
    });
    queueCliAnalyticsEvent(ctx, "agent_proposal_approved", {
      agent_id: agentId,
      proposal_id: res.proposal_id,
    });
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
    setCliAnalyticsContext(ctx, {
      agent_id: agentId,
      proposal_id: res.proposal_id,
      proposal_status: res.status,
      rejection_reason: res.rejection_reason,
    });
    queueCliAnalyticsEvent(ctx, "agent_proposal_rejected", {
      agent_id: agentId,
      proposal_id: res.proposal_id,
    });
    if (ctx.jsonMode) {
      printJson(ctx.stdout, res);
    } else {
      writeLine(ctx.stdout, ui.ok(`rejected ${res.proposal_id} (${res.rejection_reason})`));
    }
    return 0;
  }

  if (subaction === "veto") {
    const reason = parsed.options.reason || null;
    const res = await request(
      ctx,
      `${AGENTS_PATH}${encodeURIComponent(agentId)}/proposals/${encodeURIComponent(proposalId)}:veto`,
      {
        method: "POST",
        headers: apiHeaders(ctx),
        body: JSON.stringify(reason ? { reason } : {}),
      },
    );
    setCliAnalyticsContext(ctx, {
      agent_id: agentId,
      proposal_id: res.proposal_id,
      proposal_status: res.status,
      rejection_reason: res.rejection_reason,
    });
    queueCliAnalyticsEvent(ctx, "agent_proposal_vetoed", {
      agent_id: agentId,
      proposal_id: res.proposal_id,
    });
    if (ctx.jsonMode) {
      printJson(ctx.stdout, res);
    } else {
      writeLine(ctx.stdout, ui.ok(`vetoed ${res.proposal_id} (${res.rejection_reason})`));
    }
    return 0;
  }

  // non-JSON: preserve multi-line usage text not expressible as a single message
  if (ctx.jsonMode) {
    writeError(ctx, "UNKNOWN_COMMAND", `unknown proposals subaction: ${subaction}`, deps);
  } else {
    writeLine(ctx.stderr, ui.err("usage: agent proposals <agent-id>"));
    writeLine(ctx.stderr, ui.err("       agent proposals <agent-id> approve <proposal-id>"));
    writeLine(ctx.stderr, ui.err("       agent proposals <agent-id> reject <proposal-id> [--reason TEXT]"));
    writeLine(ctx.stderr, ui.err("       agent proposals <agent-id> veto <proposal-id> [--reason TEXT]"));
  }
  return 2;
}

function describeProposalAction(item) {
  if (item?.status === "VETO_WINDOW") {
    return `${formatCountdown(item.auto_apply_at)} - zombiectl agent proposals veto ${item.proposal_id} to cancel`;
  }
  return "manual review required";
}

function formatCountdown(autoApplyAt) {
  if (!Number.isFinite(autoApplyAt)) return "Auto-apply scheduled";
  const diffMs = Math.max(0, autoApplyAt - Date.now());
  const totalMinutes = Math.floor(diffMs / 60000);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return `Auto-applies in ${hours}h ${minutes}m`;
}
