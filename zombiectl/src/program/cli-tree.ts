// Single source of truth for the zombiectl command tree. buildProgram
// returns a configured commander.Command — cli.js wires creds, ctx,
// analytics, and the preAction auth-guard around it. Pure construction;
// no I/O at module load.
//
// Each .action() callback constructs `parsed = { options, positionals }`
// from commander's parsed opts + args so the existing leaf handlers
// (which already accept that shape) keep their internal signatures.
// Option validators come from validators.ts and throw
// InvalidArgumentError on rejection, which commander catches and
// renders as `error: option '--foo <v>' argument '<x>' is invalid. <why>`
// then exits 2.

import { Command, type Help } from "commander";
import { ZombieHelp, styleTagline } from "./help.ts";
import { parseIntOption, parseIdOption } from "./validators.ts";
import { buildZombieTree } from "./cli-tree-zombie.ts";
import type {
  ActionFrame,
  BuildProgramOptions,
  CommandHandlerFn,
  Handlers,
  ProgramState,
} from "./cli-tree-types.ts";
import type { ParsedArgs } from "../commands/types.ts";

const TIMEOUT_SEC_BOUNDS = { min: 1, max: 3600 };
// Floor (1ms) is the validator's job; the handler enforces a hard
// 500ms minimum sleep at runtime. Keeping the validator permissive
// lets fast integration tests pass `--poll-ms 50` without commander
// rejecting it before reaching the handler.
const POLL_MS_BOUNDS = { min: 1, max: 60_000 };
const BILLING_LIMIT_BOUNDS = { min: 1, max: 100 };

function helpTail(): string {
  // Commander's default help shows top-level commands only — subcommand
  // names (`workspace add`, `workspace list`, …) and the env-var matrix
  // never appear in the top-level body. Operators (and acceptance tests)
  // expect both. addHelpText is additive — it does not override
  // formatHelp, so commander still owns the layout above.
  const subcommands = [
    "auth status",
    "workspace add", "workspace list", "workspace use", "workspace show",
    "workspace credentials", "workspace delete",
    "agent add", "agent list", "agent delete",
    "grant list", "grant delete",
    "tenant provider show", "tenant provider add", "tenant provider delete",
    "billing show",
    "credential add", "credential show", "credential list", "credential delete",
    "zombie update",
  ];
  return [
    "",
    "Subcommands:",
    ...subcommands.map((s) => `  ${s}`),
    "",
    "Environment variables:",
    "  ZOMBIE_API_URL    API base URL (overridden by --api)",
    "  ZOMBIE_TOKEN      Auth token (overridden by login)",
    "  ZOMBIE_API_KEY    API key for service auth",
    "  ZOMBIE_STATE_DIR  Directory for local CLI state files",
    "  NO_COLOR          Any non-empty value disables color",
  ].join("\n");
}

function normalizeOptions(opts: Record<string, unknown>): Record<string, unknown> {
  // Commander camelCases hyphenated flag names: `--workspace-id` → `opts.workspaceId`.
  // The OPT_* constants in src/constants/cli-flags.ts carry the dashed
  // wire-form (`"workspace-id"`), so leaf handlers reading
  // `parsed.options[OPT_WORKSPACE_ID]` only find the dashed key. Mirror
  // every camelCase key under its dashed form so both spellings resolve
  // — no handler needs a per-option shim.
  const out: Record<string, unknown> = { ...opts };
  for (const k of Object.keys(opts)) {
    const dashed = k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
    if (dashed !== k && !(dashed in out)) out[dashed] = opts[k];
  }
  return out;
}

function actionFor(
  name: string,
  fn: (frame: ActionFrame) => Promise<void>,
): (...callbackArgs: unknown[]) => Promise<void> {
  // Returns a commander action callback. `this` inside the function
  // body refers to the commander Command instance, which exposes
  // .opts() (local + inherited globals merged) and .args (positionals
  // after option stripping). The constructed `parsed` shape is the
  // same { options, positionals } object the leaf handlers consumed
  // pre-commander, so nothing downstream needs to learn commander.
  return async function action(...callbackArgs: unknown[]): Promise<void> {
    const command = callbackArgs[callbackArgs.length - 1] as Command;
    const options = normalizeOptions(
      command.optsWithGlobals() as Record<string, unknown>,
    ) as ParsedArgs["options"];
    const positionals = command.args.slice();
    const parsed: ParsedArgs = { options, positionals };
    await fn({ name, parsed, command });
  };
}

function runHandler(
  state: ProgramState,
  frame: ActionFrame,
  handler: CommandHandlerFn,
): Promise<void> {
  if (typeof handler !== "function") {
    state.exitCode = 2;
    throw new Error(`no handler wired for command: ${frame.name}`);
  }
  return Promise.resolve(handler(frame)).then((code) => {
    state.exitCode = typeof code === "number" ? code : 0;
  });
}

export function buildProgram({ handlers, version, state, helpFactory }: BuildProgramOptions): Command {
  const program = new Command();

  // commander 14: configureHelp() ignores unknown keys (incl. helpFactory);
  // the supported override is createHelp, invoked on each Command's --help.
  program.createHelp = helpFactory ?? ((): Help => new ZombieHelp());

  program
    .name("zombiectl")
    .description(styleTagline("usezombie cli"))
    .version(version, "-v, --version", "Show version")
    .helpOption("-h, --help", "Show this help")
    .showSuggestionAfterError(true)
    .showHelpAfterError("(use --help for usage)")
    .addHelpText("after", helpTail());

  // Global options. --api and --json are read by every command via
  // optsWithGlobals(); --no-input + --no-open are surfaced for the
  // commands that observe them (login, doctor).
  program
    .option("--api <url>", "API base URL")
    .option("--json", "Machine-readable JSON output", false)
    .option("--no-input", "Disable interactive prompts")
    .option("--no-open", "Skip auto-opening the browser on login");

  // ── User commands ────────────────────────────────────────────────

  program
    .command("login")
    .description("Authenticate via browser")
    .option("--timeout-sec <n>", "Wait up to N seconds for browser callback", parseIntOption(TIMEOUT_SEC_BOUNDS))
    .option("--poll-ms <n>", "Poll cadence in milliseconds", parseIntOption(POLL_MS_BOUNDS))
    .action(actionFor("login", (frame) => runHandler(state, frame, handlers.login)));

  program
    .command("logout")
    .description("Clear stored credentials")
    .action(actionFor("logout", (frame) => runHandler(state, frame, handlers.logout)));

  const auth = program.command("auth").description("Inspect authentication state");
  auth
    .command("status")
    .description("Show active token source, claims, and server-side validity")
    .action(actionFor("auth.status", (frame) => runHandler(state, frame, handlers.auth.status)));

  program
    .command("doctor")
    .description("Diagnose CLI configuration and connectivity")
    .action(actionFor("doctor", (frame) => runHandler(state, frame, handlers.doctor)));

  buildWorkspaceTree(program, handlers, state);
  buildAgentTree(program, handlers, state);
  buildGrantTree(program, handlers, state);
  buildTenantTree(program, handlers, state);
  buildBillingTree(program, handlers, state);
  buildZombieTree(program, handlers, state, { actionFor, runHandler });

  return program;
}

function buildWorkspaceTree(program: Command, handlers: Handlers, state: ProgramState): void {
  const ws = program
    .command("workspace")
    .description("Manage workspaces");

  ws.command("add [name]")
    .description("Create a new workspace")
    .action(actionFor("workspace.add", (frame) => runHandler(state, frame, handlers.workspace.add)));

  ws.command("list")
    .description("List workspaces")
    .action(actionFor("workspace.list", (frame) => runHandler(state, frame, handlers.workspace.list)));

  ws.command("use <workspace_id>")
    .description("Set the active workspace")
    .action(actionFor("workspace.use", (frame) => runHandler(state, frame, handlers.workspace.use)));

  ws.command("show [workspace_id]")
    .description("Show workspace details")
    .option("--workspace-id <id>", "Workspace ID (alternative to positional)", parseIdOption)
    .action(actionFor("workspace.show", (frame) => runHandler(state, frame, handlers.workspace.show)));

  ws.command("credentials")
    .description("Open the workspace credential vault")
    .action(actionFor("workspace.credentials", (frame) => runHandler(state, frame, handlers.workspace.credentials)));

  ws.command("delete <workspace_id>")
    .description("Delete a workspace (irreversible)")
    .action(actionFor("workspace.delete", (frame) => runHandler(state, frame, handlers.workspace.delete)));
}

function buildAgentTree(program: Command, handlers: Handlers, state: ProgramState): void {
  const agent = program
    .command("agent")
    .description("Manage external agent API keys");

  agent.command("add")
    .description("Mint an agent API key for the workspace")
    .option("--workspace <id>", "Workspace ID", parseIdOption)
    .option("--zombie <id>", "Zombie ID this key is bound to", parseIdOption)
    .option("--name <name>", "Human-readable agent name")
    .option("--description <desc>", "Optional description")
    .action(actionFor("agent.add", (frame) => runHandler(state, frame, handlers.agent.add)));

  agent.command("list")
    .description("List external agent API keys")
    .option("--workspace <id>", "Workspace ID", parseIdOption)
    .action(actionFor("agent.list", (frame) => runHandler(state, frame, handlers.agent.list)));

  agent.command("delete <agent_id>")
    .description("Revoke an external agent API key")
    .option("--workspace <id>", "Workspace ID", parseIdOption)
    .action(actionFor("agent.delete", (frame) => runHandler(state, frame, handlers.agent.delete)));
}

function buildGrantTree(program: Command, handlers: Handlers, state: ProgramState): void {
  const grant = program
    .command("grant")
    .description("Manage integration grants");

  grant.command("list")
    .description("List integration grants for a zombie")
    .option("--zombie <id>", "Zombie ID", parseIdOption)
    .action(actionFor("grant.list", (frame) => runHandler(state, frame, handlers.grant.list)));

  grant.command("delete <grant_id>")
    .description("Revoke an integration grant")
    .option("--zombie <id>", "Zombie ID", parseIdOption)
    .action(actionFor("grant.delete", (frame) => runHandler(state, frame, handlers.grant.delete)));
}

function buildTenantTree(program: Command, handlers: Handlers, state: ProgramState): void {
  const tenant = program
    .command("tenant")
    .description("Tenant-scoped commands");
  const provider = tenant
    .command("provider")
    .description("Manage tenant LLM provider posture");

  provider.command("show")
    .description("Show the active provider config")
    .action(actionFor("tenant.provider.show", (frame) => runHandler(state, frame, handlers.tenant.provider.show)));

  provider.command("add")
    .description("Use a self-managed credential")
    .option("--credential <name>", "Named credential from the workspace vault")
    .option("--model <name>", "Override the default model identifier")
    .action(actionFor("tenant.provider.add", (frame) => runHandler(state, frame, handlers.tenant.provider.add)));

  provider.command("delete")
    .description("Reset to the platform default")
    .action(actionFor("tenant.provider.delete", (frame) => runHandler(state, frame, handlers.tenant.provider.delete)));
}

function buildBillingTree(program: Command, handlers: Handlers, state: ProgramState): void {
  const billing = program
    .command("billing")
    .description("Tenant billing dashboard");

  billing.command("show")
    .description("Plan, balance, and recent events")
    .option("--limit <n>", "Number of recent events to show", parseIntOption(BILLING_LIMIT_BOUNDS))
    .option("--cursor <token>", "next_cursor from a previous page")
    .action(actionFor("billing.show", (frame) => runHandler(state, frame, handlers.billing.show)));
}
