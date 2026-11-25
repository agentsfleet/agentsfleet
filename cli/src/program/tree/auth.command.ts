// Who this terminal is signed in as, and the two verbs that change it.

import { Command, Flag } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { whoamiEffect } from "../../commands/auth/whoami.ts";
import { logoutEffect } from "../../commands/auth/logout.ts";
import { loginEffectFromFlags } from "../../commands/auth/login.ts";
import { forceFlag, logoutAllFlag, opt, tokenNameFlag } from "./flags.ts";

// What logout actually does, at its true scope: it revokes THIS machine's
// credential and aborts unfinished sign-ins. Other machines stay signed in,
// and the description says so, because a person deciding whether a lost
// laptop is signed out reads exactly this line.
const LOGOUT_DESCRIPTION =
  "Sign out on this computer (other computers stay signed in)" as const;

const loginCommand = Command.make("login", {
  tokenName: tokenNameFlag,
  force: forceFlag,
  noOpen: Flag.Boolean("no-open").pipe(
    Flag.withDescription("Don't open the browser on its own"),
    Flag.withDefault(false),
  ),
  noInput: Flag.Boolean("no-input").pipe(
    Flag.withDescription("Never stop to ask you a question"),
    Flag.withDefault(false),
  ),
}).pipe(
  Command.withDescription("Sign in through your browser"),
  guardedHandler(({ tokenName, force, noOpen, noInput }) =>
    loginEffectFromFlags({ noOpen, noInput, force, tokenName: opt(tokenName) }),
  ),
);

const logoutCommand = Command.make("logout", { all: logoutAllFlag }).pipe(
  Command.withDescription(LOGOUT_DESCRIPTION),
  guardedHandler(({ all }) => logoutEffect({ all })),
);

const whoamiCommand = Command.make("whoami").pipe(
  Command.withDescription("Show which account you are signed in as"),
  guardedHandler(() => whoamiEffect),
);

export const authCommands = () => [loginCommand, logoutCommand, whoamiCommand] as const;
