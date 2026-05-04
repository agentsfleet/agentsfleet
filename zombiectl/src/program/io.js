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
  writeLine(stdout, "  workspace add [<name>]");
  writeLine(stdout, "  workspace list");
  writeLine(stdout, "  workspace use <workspace_id>");
  writeLine(stdout, "  workspace show [--workspace-id ID]");
  writeLine(stdout, "  workspace credentials");
  writeLine(stdout, "  workspace delete <workspace_id>");
  writeLine(stdout, "  doctor");
  writeLine(stdout);
  writeLine(stdout, ui.head("ZOMBIE COMMANDS  (top-level — e.g. `zombiectl list`)"));
  writeLine(stdout, "  install --from <path>               Register the zombie at <path>; server activates it");
  writeLine(stdout, "  list [--cursor C] [--limit N]       List zombies (cursor-paginated)");
  writeLine(stdout, "  status [<zombie_id>]                Show zombie(s) status");
  writeLine(stdout, "  stop <zombie_id>                    Halt the running session (resumable)");
  writeLine(stdout, "  resume <zombie_id>                  Resume from stopped or auto-paused");
  writeLine(stdout, "  kill <zombie_id>                    Mark terminal (irreversible)");
  writeLine(stdout, "  delete <zombie_id>                  Hard-delete (must kill first)");
  writeLine(stdout, "  logs <zombie_id>                    Tail zombie activity");
  writeLine(stdout, "  steer <zombie_id> \"<message>\"       Send a message to the zombie and stream its response");
  writeLine(stdout, "  credential add|show|list|delete     Workspace credential vault (--data=@- pipes JSON on stdin)");

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
  writeLine(stdout, "  NO_COLOR         Set to 1 to disable color output");
}

export {
  printHelp,
  printJson,
  writeError,
  writeLine,
};
