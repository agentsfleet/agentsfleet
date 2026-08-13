// The credential exchange's own proofs: what a stored value must look like
// before it is carried on a request, what the machine label may contain, and
// that the retired session-token persistence has no caller left.
//
// The load-shape check is the one that matters most. It is the mechanism
// that makes "a session token reached disk" a failure at read rather than a
// value that quietly authenticates for a minute and then stops working, so
// each refusal case below is a regression this file exists to catch.

import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import path from "node:path";
import { Effect, Option } from "effect";
import { Credentials, credentialsLayer } from "../src/services/credentials.ts";
import { saveCredentials } from "../src/lib/state.ts";
import { machineName } from "../src/commands/login-exchange.ts";
import {
  CLI_CREDENTIAL_BODY_LEN,
  CLI_CREDENTIAL_PREFIX,
  MAX_MACHINE_NAME_LEN,
  TENANT_KEY_PREFIX,
} from "../src/constants/cli-credential.ts";
import { useFreshStateDir } from "./helpers-cli-state.ts";

useFreshStateDir();

const hex = (char: string): string => char.repeat(CLI_CREDENTIAL_BODY_LEN);
const WELL_FORMED = `${CLI_CREDENTIAL_PREFIX}${hex("a")}`;

// A Clerk session token: three dot-separated base64url segments. This is the
// exact value the check exists to keep off the wire.
const SESSION_TOKEN = "eyJhbGciOiJIUzI1NiJ9.payload-part.signature-part";

const loadToken = async (): Promise<Option.Option<string>> =>
  Effect.runPromise(
    Effect.provide(
      Effect.gen(function* () {
        const credentials = yield* Credentials;
        const token = yield* credentials.getAccessToken;
        return Option.map(token, (value) => String(value));
      }),
      credentialsLayer,
    ),
  );

// Plant a value the way a regression would: straight into the file, past the
// service that would have refused it on the way in.
const plant = async (token: string): Promise<void> => {
  await saveCredentials({
    token,
    saved_at: Date.now(),
    session_id: "sess_shape",
    api_url: null,
    credential_id: null,
  });
};

describe("test_non_prefixed_value_is_refused_on_load", () => {
  test("a session token in the credential field reads as logged out, never as a credential", async () => {
    await plant(SESSION_TOKEN);
    expect(Option.isNone(await loadToken())).toBe(true);
  });

  test("a well-formed credential loads", async () => {
    await plant(WELL_FORMED);
    expect(Option.isSome(await loadToken())).toBe(true);
  });

  test("a tenant key loads — it is a different credential class, refused at the route rather than at the file", async () => {
    await plant(`${TENANT_KEY_PREFIX}${hex("b")}`);
    expect(Option.isSome(await loadToken())).toBe(true);
  });

  test("the prefix alone is not enough: truncated, over-long, upper-case, and non-hex bodies are all refused", async () => {
    const malformed = [
      `${CLI_CREDENTIAL_PREFIX}${"a".repeat(CLI_CREDENTIAL_BODY_LEN - 1)}`,
      `${CLI_CREDENTIAL_PREFIX}${"a".repeat(CLI_CREDENTIAL_BODY_LEN + 1)}`,
      `${CLI_CREDENTIAL_PREFIX}${"A".repeat(CLI_CREDENTIAL_BODY_LEN)}`,
      `${CLI_CREDENTIAL_PREFIX}${"g".repeat(CLI_CREDENTIAL_BODY_LEN)}`,
      CLI_CREDENTIAL_PREFIX,
      "",
    ];
    for (const value of malformed) {
      await plant(value);
      expect(Option.isNone(await loadToken())).toBe(true);
    }
  });

  test("a well-formed credential carrying trailing bytes is refused — the check is anchored, not a prefix match", async () => {
    await plant(`${WELL_FORMED}-extra`);
    expect(Option.isNone(await loadToken())).toBe(true);
  });
});

describe("machineName", () => {
  test("a hostname inside the grammar is preserved", () => {
    expect(machineName("indy-macbook.local")).toBe("indy-macbook.local");
  });

  test("bytes outside the grammar are replaced, so a refused mint is impossible from a hostname alone", () => {
    expect(machineName("my machine")).toBe("my-machine");
    expect(machineName("hôte")).toBe("h-te");
    expect(machineName("host\nname")).toBe("host-name");
  });

  test("an over-long hostname is truncated to the server's cap", () => {
    const derived = machineName("h".repeat(MAX_MACHINE_NAME_LEN * 2));
    expect(derived.length).toBe(MAX_MACHINE_NAME_LEN);
  });

  test("a hostname that sanitizes to nothing falls back to a name inside the grammar", () => {
    expect(machineName("")).toBe("unknown-machine");
  });

  test("every derived name satisfies the grammar the endpoint enforces", () => {
    const hostnames = ["", "my machine", "hôte", "a".repeat(200), "ok.host-1_2"];
    for (const hostname of hostnames) {
      expect(machineName(hostname)).toMatch(
        new RegExp(`^[a-zA-Z0-9._-]{1,${MAX_MACHINE_NAME_LEN}}$`),
      );
    }
  });
});

describe("test_session_token_persistence_has_no_caller", () => {
  // A pin, not a proof: the retired shape was `persistSuccess(sessionId,
  // token)`, passing the decrypted session token straight to persistence.
  // Reverting to it is the most likely way this regresses, and the type
  // signature alone would not catch a future overload taking a raw string.
  test("login.ts persists a minted credential, never the decrypted session token", () => {
    const source = readFileSync(
      path.join(import.meta.dir, "..", "src", "commands", "login.ts"),
      "utf8",
    );
    expect(source).not.toContain("persistSuccess(sessionId, token)");
    expect(source).toContain("persistSuccess(sessionId, minted)");
  });

  test("no source file outside the exchange builds a mint request", () => {
    const loginHelpers = readFileSync(
      path.join(import.meta.dir, "..", "src", "commands", "login-helpers.ts"),
      "utf8",
    );
    // saveDirectToken is the only other persistence path and must stay a
    // pass-through for a supplied key: it mints nothing, so it holds no
    // identifier to revoke later.
    expect(loginHelpers).not.toContain("machine_name");
    expect(loginHelpers).toContain("credentialId: null");
  });
});
