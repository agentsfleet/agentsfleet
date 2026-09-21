// `secret create` / `secret update` on the typed form — the shape engaged by
// --api-key and --model without --provider, plus the error prose each wrong
// pairing earns. These assert what the caller is TOLD, not merely that the
// call was refused.

import { describe, test, expect } from "bun:test";
import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";
import { OPENAI_COMPATIBLE_PROVIDER, SECRET_FIELD_API_KEY } from "../src/constants/custom-endpoint.ts";
import {
  CATALOGUE_PAGE,
  WS_ID,
  SECRET_NAME,
  VALID_BASE_URL,
  API_KEY,
  MODEL,
  authedScope,
} from "./helpers-custom-secret.ts";

describe("secret create — the typed form and its rejections", () => {
  // The catalogue parser only runs on a flag the invocation actually CARRIES,
  // so the closed catalogue is worth nothing on an invocation that omits
  // --provider. The typed form is engaged by --api-key/--model alone, and the
  // composed body would carry `provider: ""` — which the server classifies as
  // a provider_key like any other non-sentinel string. Stored, reported
  // stored, never dialable: the exact failure the closed flag exists to stop.
  for (const verb of ["create", "update"] as const) {
    test(`secret ${verb}: --api-key with --model and no --provider is refused before the network`, async () => {
      await authedScope(async () => {
        const routes: MockRoutes = {
          [`GET /v1/workspaces/${WS_ID}/secrets`]: () =>
            jsonResponse(200, { secrets: [] }),
          [`POST /v1/workspaces/${WS_ID}/secrets`]: () =>
            jsonResponse(201, { name: SECRET_NAME }),
          [`PUT /v1/workspaces/${WS_ID}/secrets/${SECRET_NAME}`]: () =>
            jsonResponse(200, { name: SECRET_NAME }),
        };
        await withMockApi(routes, async (apiUrl, calls) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            [
              "secret", verb, SECRET_NAME,
              "--api-key", API_KEY,
              "--model", MODEL,
              "--json",
            ],
            { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
          );
          expect(code).not.toBe(0);
          expect(calls.filter((c) => c.method !== "GET")).toHaveLength(0);
          const text = out.read() + err.read();
          expect(text).toMatch(/requires --provider/i);
        });
      });
    });
  }

  test("a named-provider error never recommends the custom-endpoint form", async () => {
    // The usage line appended to a failure has to be runnable as printed. One
    // shared usage string meant a named-provider error recommended
    // `--provider openai-compatible --base-url …`; following it produced
    // `--base-url is only valid with --provider openai-compatible` — advice
    // whose only outcome is the next error.
    await authedScope(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(
        ["secret", "create", SECRET_NAME, "--provider", "anthropic", "--model", MODEL, "--json"],
        { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: "http://127.0.0.1:1/" }) },
      );
      expect(code).not.toBe(0);
      const text = out.read() + err.read();
      expect(text).toContain("--api-key");
      expect(text).not.toContain("--base-url");
      expect(text).not.toContain(OPENAI_COMPATIBLE_PROVIDER);
    });
  });

  test("the missing --model error prints the usage for the shape the caller is in", async () => {
    // usageFor(isCustom) has two arms and neither was pinned: inverting the
    // ternary left the whole suite green while every missing-model error
    // printed a usage line the caller could not run.
    await authedScope(async () => {
      const custom = bufferStream();
      await runCli(
        ["secret", "create", SECRET_NAME, "--provider", OPENAI_COMPATIBLE_PROVIDER,
          "--base-url", VALID_BASE_URL, "--api-key", API_KEY, "--json"],
        { stdout: custom.stream, stderr: custom.stream, env: cliEnv({ AGENTSFLEET_API_URL: "http://127.0.0.1:1/" }) },
      );
      expect(custom.read()).toContain("--base-url https://host/v1");

      const named = bufferStream();
      await runCli(
        ["secret", "create", SECRET_NAME, "--provider", "anthropic", "--api-key", API_KEY, "--json"],
        { stdout: named.stream, stderr: named.stream, env: cliEnv({ AGENTSFLEET_API_URL: "http://127.0.0.1:1/" }) },
      );
      const n = named.read();
      expect(n).toMatch(/--model/i);
      expect(n).not.toContain("--base-url");
      expect(n).not.toContain(OPENAI_COMPATIBLE_PROVIDER);
    });
  });

  test("a whitespace-only --api-key is refused, not stored as blank", async () => {
    // --api-key was the one typed flag that did not trim, so "   " passed the
    // non-empty gate and was stored verbatim: the vault reports success, the
    // resolver probe sees a non-empty key, and the credential can never
    // authenticate — the same store-succeeds/never-dials failure the closed
    // --provider flag exists to prevent, reached through the sibling flag.
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () => jsonResponse(200, { secrets: [] }),
        [`POST /v1/workspaces/${WS_ID}/secrets`]: () => jsonResponse(201, { name: SECRET_NAME }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["secret", "create", SECRET_NAME, "--provider", "anthropic",
            "--api-key", "   ", "--model", MODEL, "--json"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).not.toBe(0);
        expect(calls.filter((c) => c.method === "POST")).toHaveLength(0);
        expect(out.read() + err.read()).toContain("--api-key");
      });
    });
  });

  test("a padded --api-key is stored trimmed, never with its padding", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () => jsonResponse(200, { secrets: [] }),
        [`POST /v1/workspaces/${WS_ID}/secrets`]: () => jsonResponse(201, { name: SECRET_NAME }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const code = await runCli(
          ["secret", "create", SECRET_NAME, "--provider", "anthropic",
            "--api-key", `  ${API_KEY}  `, "--model", MODEL, "--json"],
          { stdout: out.stream, stderr: bufferStream().stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const post = calls.find((c) => c.method === "POST");
        const sent = JSON.parse(post?.body ?? "{}") as { data?: Record<string, unknown> };
        expect(sent.data?.[SECRET_FIELD_API_KEY]).toBe(API_KEY);
      });
    });
  });

  test("a CLI-engine provider is refused by name, with the reason — not the accepted-set wall", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        "GET /v1/models": () => jsonResponse(200, CATALOGUE_PAGE),
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(200, { secrets: [] }),
        [`POST /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(201, { name: SECRET_NAME }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          [
            "secret", "create", SECRET_NAME,
            "--provider", "claude-cli",
            "--api-key", API_KEY,
            "--model", MODEL,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).not.toBe(0);
        // The catalogue read is expected; the credential POST is not.
        expect(calls.some((c) => c.method === "POST")).toBe(false);
        const text = out.read() + err.read();
        expect(text).toContain("claude-cli");
        expect(text).toContain("carries no API key");
        // The reason replaces the wall; printing both would bury it.
        expect(text).not.toMatch(/is not in this server's model catalogue/i);
      });
    });
  });

  test("--data and the typed flags together are rejected (mutually exclusive)", async () => {
    await authedScope(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(
        [
          "secret", "create", SECRET_NAME,
          "--provider", OPENAI_COMPATIBLE_PROVIDER,
          "--base-url", VALID_BASE_URL,
          "--api-key", API_KEY,
          "--data", '{"x":1}',
          "--json",
        ],
        { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: "http://127.0.0.1:1/" }) },
      );
      expect(code).not.toBe(0);
      const text = out.read() + err.read();
      expect(text).toMatch(/--data|both/i);
    });
  });
});
