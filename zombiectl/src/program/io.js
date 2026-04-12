import { printBanner } from "./banner.js";

function writeLine(stream, line = "") {
  stream.write(`${line}\n`);
}

function printJson(stream, value) {
  writeLine(stream, JSON.stringify(value, null, 2));
}

function writeError(ctx, code, message, { printJson: pj, writeLine: wl, ui: u }) {
  if (ctx.jsonMode) {
    (pj || printJson)(ctx.stderr, { error: { code, message } });
  } else {
    (wl || writeLine)(ctx.stderr, (u || { err: (s) => s }).err(message));
  }
}

function printHelp(stdout, ui, opts = {}) {
  const version = opts.version || "0.1.0";
  const noColor = Boolean(opts.env?.NO_COLOR === "1" || opts.env?.NO_COLOR === "true");
  const jsonMode = opts.jsonMode || false;
  const showOperator = Boolean(opts.env?.ZOMBIE_OPERATOR === "1" || opts.authRole === "operator" || opts.authRole === "admin" || opts.operator);

  printBanner(stdout, version, { noColor, jsonMode });
  writeLine(stdout);
  writeLine(stdout, "  " + ui.head("UseZombie CLI") + ui.dim("  —  autonomous agent platform"));
  writeLine(stdout);
  writeLine(stdout, ui.head("USAGE"));
  writeLine(stdout, "  zombiectl [--api URL] [--json] <command> [subcommand] [flags]");
  writeLine(stdout);
  writeLine(stdout, ui.head("USER COMMANDS"));
  writeLine(stdout, "  login [--timeout-sec N] [--poll-ms N] [--no-open]");
  writeLine(stdout, "  logout");
  writeLine(stdout, "  workspace add <repo_url> [--default-branch BRANCH]");
  writeLine(stdout, "  workspace list");
  writeLine(stdout, "  workspace remove <workspace_id>");
  writeLine(stdout, "  spec init [--path DIR] [--output PATH]");
  writeLine(stdout, "  specs sync [--workspace-id ID]");
  writeLine(stdout, "  run [--workspace-id ID] [--spec-id ID] [--mode MODE] [--requested-by USER] [--idempotency-key KEY]");
  writeLine(stdout, "  run [--spec FILE] [--preview] [--preview-only] [--path DIR]");
  writeLine(stdout, "  run status <run_id>");
  writeLine(stdout, "  runs list [--workspace-id ID] [--limit N] [--starting-after ID]");
  writeLine(stdout, "  doctor");

  if (showOperator) {
    writeLine(stdout);
    writeLine(stdout, ui.head("OPERATOR COMMANDS"));
    writeLine(stdout, "  workspace upgrade-scale --workspace-id ID --subscription-id SUBSCRIPTION_ID");
    writeLine(stdout, "  skill-secret put --workspace-id ID --skill-ref REF --key KEY --value VALUE [--scope host|sandbox]");
    writeLine(stdout, "  skill-secret delete --workspace-id ID --skill-ref REF --key KEY");
    writeLine(stdout, "  agent scores <agent-id> [--limit N] [--starting-after ID]");
    writeLine(stdout, "  agent profile <agent-id>");
    writeLine(stdout, "  agent improvement-report <agent-id>");
    writeLine(stdout, "  agent proposals <agent-id>");
    writeLine(stdout, "  agent proposals <agent-id> approve <proposal-id>");
    writeLine(stdout, "  agent proposals <agent-id> reject <proposal-id> [--reason TEXT]");
    writeLine(stdout, "  agent proposals <agent-id> veto <proposal-id> [--reason TEXT]");
    writeLine(stdout, "  admin config set scoring_context_max_tokens <value> --workspace-id ID");
  }

  writeLine(stdout);
  writeLine(stdout, ui.head("GLOBAL FLAGS"));
  writeLine(stdout, "  --api URL        API base URL");
  writeLine(stdout, "  --json           Machine-readable JSON output");
  writeLine(stdout, "  --no-input       Disable interactive prompts");
  writeLine(stdout, "  --no-open        Skip auto-opening browser");
  writeLine(stdout, "  --version        Show version");
  writeLine(stdout, "  --help, -h       Show this help");

  writeLine(stdout);
  writeLine(stdout, ui.head("ENVIRONMENT VARIABLES"));
  writeLine(stdout, "  ZOMBIE_API_URL   API base URL (overridden by --api)");
  writeLine(stdout, "  ZOMBIE_TOKEN     Auth token (overridden by login)");
  writeLine(stdout, "  ZOMBIE_API_KEY   API key for service auth");
  writeLine(stdout, "  ZOMBIE_OPERATOR  Set to 1 to force-show operator commands in help");
  writeLine(stdout, "  NO_COLOR         Set to 1 to disable color output");
  writeLine(stdout);
  writeLine(stdout, ui.dim("workspace add opens UseZombie GitHub App install and binds via callback."));
}

export {
  printHelp,
  printJson,
  writeError,
  writeLine,
};
