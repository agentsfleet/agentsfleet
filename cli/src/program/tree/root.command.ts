// The whole command tree, assembled.
//
// The groups live in sibling files because each is long enough to own one, and
// because the LENGTH GATE would otherwise be the thing that decided when a new
// verb could land.
//
// The global flags are declared here so `--help` lists them and an unknown-flag
// error does not fire on them. Their VALUES are read off argv by the entry
// point before this tree runs, because they configure the layer the tree is
// provided with — `--api` picks the server every command talks to, and a flag
// cannot configure the thing that is about to execute it.

import { Option } from "effect";
import { Command, Flag } from "effect/unstable/cli";
import { doctorEffect } from "../../commands/core-ops.ts";
import { whoamiEffect } from "../../commands/whoami.ts";
import { logoutEffect } from "../../commands/auth-logout.ts";
import { loginEffectFromFlags } from "../../commands/login.ts";
import {
  apiKeyCommand,
  approvalsCommand,
  authCommand,
  connectorCommand,
  grantCommand,
} from "./access.command.ts";
import { billingCommand, tenantCommand, workspaceCommand } from "./workspace.command.ts";
import {
  deleteCommand,
  eventsCommand,
  fleetCommand,
  installCommand,
  killCommand,
  libraryCommand,
  listCommand,
  logsCommand,
  modelsCommand,
  resumeCommand,
  secretCommand,
  statusCommand,
  steerCommand,
  stopCommand,
} from "./fleet.command.ts";
import { scheduleCommand } from "./schedule.command.ts";
import { memoryCommand } from "./memory.command.ts";
import { forceFlag, logoutAllFlag, tokenNameFlag } from "./flags.ts";

const opt = Option.getOrUndefined;

const CLI_NAME = "agentsfleet" as const;

// What logout actually does, at its true scope. It revokes THIS machine's
// credential and aborts unfinished sign-ins; the help once promised "every
// active session on this account", which the daemon's own endpoint
// contradicts. Someone read that sentence and believed a laptop they had lost
// was signed out, and it was not.
const LOGOUT_DESCRIPTION =
  "Sign out — revoke this machine's credential, abort unfinished sign-ins, and clear local state (other machines stay signed in)" as const;

// Two flags are genuinely global: `--api` names the server every command talks
// to, and `--json` picks the register every command answers in. The other
// three are not — `--no-open` and `--no-input` are login's, `--tty` is steer's
// — so each is declared on the command that reads it. Declaring a flag on both
// a parent and a child is refused outright: the parent would always claim it,
// and the child's copy would silently never fire.
const globalFlags = {
  api: Flag.String("api").pipe(Flag.withDescription("API base URL"), Flag.optional),
  json: Flag.Boolean("json").pipe(Flag.withDescription("Machine-readable JSON output")),
} as const;

const loginCommand = Command.make("login", {
  tokenName: tokenNameFlag,
  force: forceFlag,
  noOpen: Flag.Boolean("no-open").pipe(
    Flag.withDescription("Skip auto-opening the browser on login"),
  ),
  noInput: Flag.Boolean("no-input").pipe(
    Flag.withDescription("Disable interactive prompts"),
  ),
}).pipe(
  Command.withDescription("Authenticate via browser"),
  Command.withHandler(({ tokenName, force, noOpen, noInput }) =>
    loginEffectFromFlags({
      noOpen,
      noInput,
      force,
      tokenName: opt(tokenName),
    }),
  ),
);

const logoutCommand = Command.make("logout", { all: logoutAllFlag }).pipe(
  Command.withDescription(LOGOUT_DESCRIPTION),
  Command.withHandler(({ all }) => logoutEffect({ all })),
);

const whoamiCommand = Command.make("whoami").pipe(
  Command.withDescription("Show who this terminal is signed in as"),
  Command.withHandler(() => whoamiEffect),
);

const doctorCommand = Command.make("doctor").pipe(
  Command.withDescription("Diagnose CLI configuration and connectivity"),
  Command.withHandler(() => doctorEffect),
);

export const rootCommand = Command.make(CLI_NAME).pipe(
  Command.withDescription("agentsfleet cli"),
  Command.withSharedFlags(globalFlags),
  Command.withSubcommands([
    loginCommand,
    logoutCommand,
    whoamiCommand,
    authCommand,
    doctorCommand,
    workspaceCommand,
    grantCommand,
    apiKeyCommand,
    connectorCommand,
    approvalsCommand,
    tenantCommand,
    billingCommand,
    libraryCommand,
    modelsCommand,
    installCommand,
    fleetCommand,
    listCommand,
    statusCommand,
    stopCommand,
    resumeCommand,
    killCommand,
    deleteCommand,
    logsCommand,
    eventsCommand,
    steerCommand,
    secretCommand,
    scheduleCommand,
    memoryCommand,
  ]),
);
