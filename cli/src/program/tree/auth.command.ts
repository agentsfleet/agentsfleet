// Who this terminal is signed in as, and the two verbs that change it.

import { Option } from "effect";
import { Command, Flag } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { whoamiEffect } from "../../commands/auth/whoami.ts";
import { logoutEffect } from "../../commands/auth/logout.ts";
import { loginEffectFromFlags } from "../../commands/auth/login.ts";
import { forceFlag, logoutAllFlag, tokenNameFlag } from "./flags.ts";

const opt = Option.getOrUndefined;

// What logout actually does, at its true scope: it revokes THIS machine's
// credential and aborts unfinished sign-ins. Other machines stay signed in,
// and the description says so, because a person deciding whether a lost
// laptop is signed out reads exactly this line.
const LOGOUT_DESCRIPTION =
  "Sign out — revoke this machine's credential, abort unfinished sign-ins, and clear local state (other machines stay signed in)" as const;

const loginCommand = Command.make("login", {
  tokenName: tokenNameFlag,
  force: forceFlag,
  noOpen: Flag.Boolean("no-open").pipe(
    Flag.withDescription("Skip auto-opening the browser on login"),
    Flag.withDefault(false),
  ),
  noInput: Flag.Boolean("no-input").pipe(
    Flag.withDescription("Disable interactive prompts"),
    Flag.withDefault(false),
  ),
}).pipe(
  Command.withDescription("Authenticate via browser"),
  guardedHandler(({ tokenName, force, noOpen, noInput }) =>
    loginEffectFromFlags({ noOpen, noInput, force, tokenName: opt(tokenName) }),
  ),
);

const logoutCommand = Command.make("logout", { all: logoutAllFlag }).pipe(
  Command.withDescription(LOGOUT_DESCRIPTION),
  guardedHandler(({ all }) => logoutEffect({ all })),
);

const whoamiCommand = Command.make("whoami").pipe(
  Command.withDescription("Show who this terminal is signed in as"),
  guardedHandler(() => whoamiEffect),
);

export const authCommands = () => [loginCommand, logoutCommand, whoamiCommand] as const;
