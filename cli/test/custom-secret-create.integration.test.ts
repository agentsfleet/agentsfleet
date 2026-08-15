// The typed secret-create form, in both of its shapes: a named provider
// (`--provider <id> --api-key <key> --model <m>`) and a custom endpoint
// (`--provider openai-compatible --base-url <url> --model <m> [--api-key
// <key>]`). Field-pairing rules mirror the resolver: --provider is required,
// model is always required; api_key is required for a named provider but
// optional for openai-compatible (keyless gateway); openai-compatible ⇔
// base_url present.
//
//   - openai-compatible + https base_url + model succeeds and POSTs a
//     secret whose `data` carries { provider, base_url, model, api_key? }.
//   - a non-https `--base-url` is rejected by the commander option validator:
//     exit non-zero, human-text stderr, and ZERO network calls — the mock's
//     `calls` ledger proves the rejection happened at PARSE time, before any
//     fetch. Full SSRF validation stays server-side (base_url_guard.zig).

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";
import {
  OPENAI_COMPATIBLE_PROVIDER,
  SECRET_FIELD_PROVIDER,
  SECRET_FIELD_API_KEY,
  SECRET_FIELD_BASE_URL,
  SECRET_FIELD_MODEL,
} from "../src/constants/custom-endpoint.ts";
// The accepted `--provider` set is now whatever GET /v1/models serves, so these
// tests state it as catalogue rows rather than importing a compiled-in list.
// Two providers is enough to prove membership, non-membership, and case-folding
// while keeping the rejection message short enough to assert on.
const CATALOGUE_PAGE = {
  version: "1",
  models: [
    // These are wire bytes the CLI parses, not values it computes.
    // pin test: literal is the contract
    { id: "claude-opus-5", provider: "anthropic", context_cap_tokens: 1000000, input_nanos_per_mtok: 5000000000, cached_input_nanos_per_mtok: 500000000, output_nanos_per_mtok: 25000000000 },
    { id: "gpt-5.6-sol", provider: "openai", context_cap_tokens: 1050000, input_nanos_per_mtok: 5000000000, cached_input_nanos_per_mtok: 500000000, output_nanos_per_mtok: 30000000000 },
  ],
  next_cursor: null,
};

const WS_ID = "ws_custom_cred_test";
const SECRET_NAME = "vllm-gateway";
const VALID_BASE_URL = "https://vllm.corp.example/v1";
const API_KEY = "sk-custom-secret-do-not-log";
const MODEL = "qwen2.5-coder";
const NON_HTTPS_BASE_URL = "http://vllm.corp.example/v1";

const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_custom_cred" }, fn);

describe("secret create — custom OpenAI-compatible endpoint", () => {
  test("openai-compatible + https base_url stores a secret carrying provider + base_url", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        // The create command lists first (upsert skip-if-exists guard) then POSTs.
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
            "--provider", OPENAI_COMPATIBLE_PROVIDER,
            "--base-url", VALID_BASE_URL,
            "--api-key", API_KEY,
            "--model", MODEL,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const parsed = JSON.parse(out.read()) as { status?: string; name?: string };
        expect(parsed.status).toBe("stored");
        expect(parsed.name).toBe(SECRET_NAME);

        const post = calls.find((c) => c.method === "POST");
        expect(post).toBeDefined();
        const sent = JSON.parse(post?.body ?? "{}") as {
          name?: string;
          data?: Record<string, unknown>;
        };
        expect(sent.name).toBe(SECRET_NAME);
        expect(sent.data?.[SECRET_FIELD_PROVIDER]).toBe(OPENAI_COMPATIBLE_PROVIDER);
        expect(sent.data?.[SECRET_FIELD_BASE_URL]).toBe(VALID_BASE_URL);
        expect(sent.data?.[SECRET_FIELD_API_KEY]).toBe(API_KEY);
        expect(sent.data?.[SECRET_FIELD_MODEL]).toBe(MODEL);
        // The secret api_key rides in the encrypted POST body but must never
        // surface on stdout (the --json success rule carries only metadata).
        expect(out.read()).not.toContain(API_KEY);
      });
    });
  });

  test("non-https --base-url is rejected by the option validator: non-zero exit, NO network call", async () => {
    await authedScope(async () => {
      // Every route is registered, so ANY request would be recorded in `calls`.
      // The validator must reject `http://` at parse time → calls stays empty.
      const routes: MockRoutes = {
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
            "--provider", OPENAI_COMPATIBLE_PROVIDER,
            "--base-url", NON_HTTPS_BASE_URL,
            "--api-key", API_KEY,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        // Commander maps InvalidArgumentError to exit 2 (usage error).
        expect(code).not.toBe(0);
        // The load-bearing assertion: the rejection happened BEFORE any fetch.
        expect(calls).toHaveLength(0);
        // Human-text stderr names the https requirement (not a JSON envelope).
        const text = out.read() + err.read();
        expect(text).toMatch(/https/i);
        expect(text).toContain("--base-url");
      });
    });
  });

  test("a malformed --base-url is rejected at parse time with no network call", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
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
            "--provider", OPENAI_COMPATIBLE_PROVIDER,
            "--base-url", "not a url",
            "--api-key", API_KEY,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).not.toBe(0);
        expect(calls).toHaveLength(0);
      });
    });
  });

  test("openai-compatible without --base-url is rejected client-side (no network)", async () => {
    await authedScope(async () => {
      // base_url omitted → the field-pairing check fails before any dispatch.
      // Point at an unroutable API to prove no request is attempted.
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(
        [
          "secret", "create", SECRET_NAME,
          "--provider", OPENAI_COMPATIBLE_PROVIDER,
          "--api-key", API_KEY,
          "--json",
        ],
        { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: "http://127.0.0.1:1/" }) },
      );
      expect(code).not.toBe(0);
      const text = out.read() + err.read();
      expect(text).toMatch(/base-url|base_url/i);
    });
  });

  test("openai-compatible WITHOUT --api-key succeeds — a keyless gateway omits the key", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
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
            "--provider", OPENAI_COMPATIBLE_PROVIDER,
            "--base-url", VALID_BASE_URL,
            "--model", MODEL,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const post = calls.find((c) => c.method === "POST");
        const sent = JSON.parse(post?.body ?? "{}") as { data?: Record<string, unknown> };
        expect(sent.data?.[SECRET_FIELD_PROVIDER]).toBe(OPENAI_COMPATIBLE_PROVIDER);
        expect(sent.data?.[SECRET_FIELD_BASE_URL]).toBe(VALID_BASE_URL);
        expect(sent.data?.[SECRET_FIELD_MODEL]).toBe(MODEL);
        // No key was passed → the body omits api_key entirely (keyless).
        expect(sent.data?.[SECRET_FIELD_API_KEY]).toBeUndefined();
      });
    });
  });

  test("a NAMED provider without --api-key is rejected (key required off the custom path)", async () => {
    await authedScope(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(
        [
          "secret", "create", SECRET_NAME,
          "--provider", "anthropic",
          "--model", MODEL,
          "--json",
        ],
        { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: "http://127.0.0.1:1/" }) },
      );
      expect(code).not.toBe(0);
      const text = out.read() + err.read();
      expect(text).toMatch(/--api-key/i);
    });
  });

  test("typed form without --model is rejected (the resolver requires a model to activate)", async () => {
    await authedScope(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(
        [
          "secret", "create", SECRET_NAME,
          "--provider", OPENAI_COMPATIBLE_PROVIDER,
          "--base-url", VALID_BASE_URL,
          "--api-key", API_KEY,
          "--json",
        ],
        { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: "http://127.0.0.1:1/" }) },
      );
      expect(code).not.toBe(0);
      const text = out.read() + err.read();
      expect(text).toMatch(/--model/i);
    });
  });

  test("a named provider carrying --base-url is rejected (no egress-widening)", async () => {
    await authedScope(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(
        [
          "secret", "create", SECRET_NAME,
          "--provider", "anthropic",
          "--base-url", VALID_BASE_URL,
          "--api-key", API_KEY,
          "--json",
        ],
        { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: "http://127.0.0.1:1/" }) },
      );
      expect(code).not.toBe(0);
      const text = out.read() + err.read();
      expect(text).toMatch(/--base-url is only valid/i);
    });
  });

  test("--model alone routes to the typed path: a clear pairing error, never 'missing --data'", async () => {
    await authedScope(async () => {
      // `--model` with no other typed flag is an incomplete custom endpoint. It
      // must route to the typed form (so the field-pairing check fires) instead
      // of falling through to the generic `--data` resolver. Point at an
      // unroutable API to prove the rejection is client-side (no network).
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(
        [
          "secret", "create", SECRET_NAME,
          "--model", MODEL,
          "--json",
        ],
        { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: "http://127.0.0.1:1/" }) },
      );
      expect(code).not.toBe(0);
      const text = out.read() + err.read();
      // The clear typed-path pairing error (names --api-key / --provider / --base-url) …
      expect(text).toMatch(/--api-key|--provider|--base-url/i);
      // … NOT the generic "missing --data" hint the old fall-through produced.
      expect(text).not.toMatch(/missing --data/i);
    });
  });

  // Commander only runs the catalogue parser on a flag it actually SEES, so
  // the closed catalogue is worth nothing on an invocation that omits
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

describe("secret create — provider catalogue closure", () => {
  test("an unknown provider is refused against the live catalogue, and never POSTed", async () => {
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
            "--provider", "notaprovider",
            "--api-key", API_KEY,
            "--model", MODEL,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).not.toBe(0);
        // The credential must never reach the vault…
        expect(calls.some((c) => c.method === "POST")).toBe(false);
        const text = out.read() + err.read();
        expect(text).toContain("notaprovider");
        // …and the accepted set names what THIS server serves, derived from the
        // catalogue rows above — not a set compiled into the binary.
        expect(text).toContain("anthropic");
        expect(text).toContain(OPENAI_COMPATIBLE_PROVIDER);
        expect(text).not.toContain("cerebras");
      });
    });
  });

  test("an unreachable catalogue accepts the provider rather than blocking the write", async () => {
    await authedScope(async () => {
      // The dashboard degrades to a free-text provider input when the catalogue
      // read fails; the CLI must degrade the same way. Refusing here would make
      // a catalogue outage mean "you may not store a credential" — a worse
      // failure than one the server rejects with a typed error.
      const routes: MockRoutes = {
        "GET /v1/models": () => jsonResponse(503, { detail: "catalogue down" }),
        [`POST /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(201, { name: SECRET_NAME }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          [
            "secret", "create", SECRET_NAME,
            "--provider", "anything-at-all",
            "--api-key", API_KEY,
            "--model", MODEL,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const post = calls.find((c) => c.method === "POST");
        expect(post).toBeDefined();
        const sent = JSON.parse(post?.body ?? "{}") as { data?: Record<string, unknown> };
        expect(sent.data?.[SECRET_FIELD_PROVIDER]).toBe("anything-at-all");
        out.read();
        err.read();
      });
    });
  });

  test("an EMPTY catalogue accepts the provider — a fresh environment stays usable", async () => {
    await authedScope(async () => {
      // `core.model_library` ships empty and the model_catalogue playbook fills
      // it. Rejecting every provider before that runs would make the CLI
      // unusable during exactly the provisioning it is used for.
      const routes: MockRoutes = {
        "GET /v1/models": () => jsonResponse(200, { version: "0", models: [] }),
        [`POST /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(201, { name: SECRET_NAME }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          [
            "secret", "create", SECRET_NAME,
            "--provider", "anthropic",
            "--api-key", API_KEY,
            "--model", MODEL,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        expect(calls.some((c) => c.method === "POST")).toBe(true);
        out.read();
        err.read();
      });
    });
  });

  test("a mixed-case catalogue member succeeds and the POSTed body carries the canonical spelling", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        // Folding is the CATALOGUE's, not the parser's: the stored body must
        // carry the spelling the resolver compares byte-for-byte, or the
        // credential reports success and can never dial.
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
            "--provider", "Anthropic",
            "--api-key", API_KEY,
            "--model", MODEL,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const post = calls.find((c) => c.method === "POST");
        const sent = JSON.parse(post?.body ?? "{}") as { data?: Record<string, unknown> };
        expect(sent.data?.[SECRET_FIELD_PROVIDER]).toBe("anthropic");
      });
    });
  });

  test("the generic --data form remains unconstrained: an out-of-catalogue provider posts verbatim", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
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
            "--data", '{"provider":"notaprovider","model":"m"}',
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const post = calls.find((c) => c.method === "POST");
        const sent = JSON.parse(post?.body ?? "{}") as { data?: Record<string, unknown> };
        expect(sent.data?.[SECRET_FIELD_PROVIDER]).toBe("notaprovider");
      });
    });
  });
});
