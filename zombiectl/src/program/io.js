import { printVersion } from "./banner.js";
import { formatHelpHeading, palette } from "../output/index.js";

const HELP_NAME_WIDTH = 26; // 2-indent + 26 + 2-gap = 30; description ≤ 50.

function writeLine(stream, line = "") {
  stream.write(`${line}\n`);
}

function printJson(stream, value) {
  writeLine(stream, JSON.stringify(value, null, 2));
}

function writeError(ctx, code, message, opts = {}) {
  const pj = opts.printJson || printJson;
  const wl = opts.writeLine || writeLine;
  const u = opts.ui || { err: (s) => s };
  if (ctx.jsonMode) {
    pj(ctx.stderr, { error: { code, message } });
  } else {
    wl(ctx.stderr, u.err(message));
  }
}

function helpRow(stdout, name, description) {
  if (!description) {
    writeLine(stdout, `  ${name}`);
    return;
  }
  const padded = name.padEnd(HELP_NAME_WIDTH);
  writeLine(stdout, `  ${padded}  ${description}`);
}

function printHelp(stdout, _ui, opts = {}) {
  const version = opts.version || "0.1.0";
  // no-color.org spec — any non-empty value disables color. Matches
  // capability.detectColorMode so an injected NO_COLOR=yes / NO_COLOR=true
  // / NO_COLOR=anything-non-empty all flow through the plain path together.
  const env = opts.env ?? {};
  const noColor = Boolean(env.NO_COLOR && env.NO_COLOR.length > 0);
  const jsonMode = opts.jsonMode || false;
  const styleOpts = { stream: stdout, env };

  // Resolve color helpers against the explicit env+stream so callers that
  // inject a custom env (tests, scripts) get coherent capability detection.
  const head = (s) => formatHelpHeading(s, styleOpts);
  const dim = (s) => palette.subtle(s, styleOpts);

  printVersion(stdout, version, { noColor, jsonMode, env });
  writeLine(stdout);
  writeLine(stdout, dim("autonomous agent platform"));
  writeLine(stdout);
  writeLine(stdout, head("USAGE"));
  writeLine(stdout, "  zombiectl [--api URL] [--json] <command> [args] [flags]");
  writeLine(stdout);
  writeLine(stdout, head("USER COMMANDS"));
  helpRow(stdout, "login [--timeout-sec N] [--poll-ms N] [--no-open]");
  helpRow(stdout, "logout");
  helpRow(stdout, "workspace add [<name>]");
  helpRow(stdout, "workspace list");
  helpRow(stdout, "workspace use <workspace_id>");
  helpRow(stdout, "workspace show [--workspace-id ID]");
  helpRow(stdout, "workspace credentials");
  helpRow(stdout, "workspace delete <workspace_id>");
  helpRow(stdout, "doctor");
  writeLine(stdout);
  writeLine(stdout, head("AGENT COMMANDS"));
  helpRow(stdout, "agent add", "Mint an agent API key for the workspace");
  helpRow(stdout, "agent list", "List agent API keys");
  helpRow(stdout, "agent delete <key_id>", "Revoke an agent API key");
  writeLine(stdout);
  writeLine(stdout, head("GRANT COMMANDS"));
  helpRow(stdout, "grant list", "List integration grants in the workspace");
  helpRow(stdout, "grant delete <grant_id>", "Revoke an integration grant");
  writeLine(stdout);
  writeLine(stdout, head("TENANT COMMANDS"));
  helpRow(stdout, "tenant provider show", "Show the BYOK provider config");
  helpRow(stdout, "tenant provider add --credential <n>", "Use a BYOK credential");
  helpRow(stdout, "tenant provider delete", "Reset to the platform default");
  writeLine(stdout);
  writeLine(stdout, head("BILLING COMMANDS"));
  helpRow(stdout, "billing show", "Plan, balance, and usage snapshot");
  writeLine(stdout);
  writeLine(stdout, head("ZOMBIE COMMANDS"));
  helpRow(stdout, "install --from <path>", "Register a zombie from <path>");
  helpRow(stdout, "list [--cursor C] [--limit N]", "List zombies (paginated)");
  helpRow(stdout, "status [<zombie_id>]", "Show zombie status");
  helpRow(stdout, "stop <zombie_id>", "Halt the session (resumable)");
  helpRow(stdout, "resume <zombie_id>", "Resume from stopped");
  helpRow(stdout, "kill <zombie_id>", "Mark terminal (irreversible)");
  helpRow(stdout, "delete <zombie_id>", "Hard-delete (kill first)");
  helpRow(stdout, "logs <zombie_id>", "Tail zombie activity");
  helpRow(stdout, "events <zombie_id> [opts]", "Page through historical events");
  helpRow(stdout, "steer <zombie_id> \"<msg>\"", "Send a message; stream response");
  helpRow(stdout, "credential add|show|list|delete", "Workspace credential vault");
  writeLine(stdout);
  writeLine(stdout, head("GLOBAL FLAGS"));
  helpRow(stdout, "--api URL", "API base URL");
  helpRow(stdout, "--json", "Machine-readable JSON output");
  helpRow(stdout, "--no-input", "Disable interactive prompts");
  helpRow(stdout, "--no-open", "Skip auto-opening browser");
  helpRow(stdout, "--version", "Show version");
  helpRow(stdout, "--help, -h", "Show this help");
  writeLine(stdout);
  writeLine(stdout, head("ENVIRONMENT VARIABLES"));
  helpRow(stdout, "ZOMBIE_API_URL", "API base URL (overridden by --api)");
  helpRow(stdout, "ZOMBIE_TOKEN", "Auth token (overridden by login)");
  helpRow(stdout, "ZOMBIE_API_KEY", "API key for service auth");
  helpRow(stdout, "NO_COLOR", "Any non-empty value disables color");
}

export {
  printHelp,
  printJson,
  writeError,
  writeLine,
};
