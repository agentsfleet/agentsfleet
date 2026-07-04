// test_cli_custom_secret_add — the typed custom-endpoint secret-add
// form (`--provider openai-compatible --base-url <url> --model <m>
// [--api-key <key>]`). Field-pairing rules mirror the resolver: model is always
// required; api_key is required for a named provider but optional for
// openai-compatible (keyless gateway); openai-compatible ⇔ base_url present.
//
//   - openai-compatible + https base_url + model succeeds and POSTs a
//     secret whose `data` carries { provider, base_url, model, api_key? }.
//   - a non-https `--base-url` is rejected by the commander option validator:
//     exit non-zero, human-text stderr, and ZERO network calls — the mock's
//     `calls` ledger proves the rejection happened at PARSE time, before any
//     fetch. Full SSRF validation stays server-side (base_url_guard.zig).

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";
import {
  OPENAI_COMPATIBLE_PROVIDER,
  SECRET_FIELD_PROVIDER,
  SECRET_FIELD_API_KEY,
  SECRET_FIELD_BASE_URL,
  SECRET_FIELD_MODEL,
} from "../src/constants/custom-endpoint.ts";

const WS_ID = "ws_custom_cred_test";
const SECRET_NAME = "vllm-gateway";
const VALID_BASE_URL = "https://vllm.corp.example/v1";
const API_KEY = "sk-custom-secret-do-not-log";
const MODEL = "qwen2.5-coder";
const NON_HTTPS_BASE_URL = "http://vllm.corp.example/v1";

const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_custom_cred" }, fn);

describe("secret add — custom OpenAI-compatible endpoint", () => {
  test("openai-compatible + https base_url stores a secret carrying provider + base_url", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        // The add command lists first (upsert skip-if-exists guard) then POSTs.
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
            "secret", "add", SECRET_NAME,
            "--provider", OPENAI_COMPATIBLE_PROVIDER,
            "--base-url", VALID_BASE_URL,
            "--api-key", API_KEY,
            "--model", MODEL,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
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
        // surface on stdout (the --json success contract carries only metadata).
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
            "secret", "add", SECRET_NAME,
            "--provider", OPENAI_COMPATIBLE_PROVIDER,
            "--base-url", NON_HTTPS_BASE_URL,
            "--api-key", API_KEY,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
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
            "secret", "add", SECRET_NAME,
            "--provider", OPENAI_COMPATIBLE_PROVIDER,
            "--base-url", "not a url",
            "--api-key", API_KEY,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
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
          "secret", "add", SECRET_NAME,
          "--provider", OPENAI_COMPATIBLE_PROVIDER,
          "--api-key", API_KEY,
          "--json",
        ],
        { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: "http://127.0.0.1:1/" } },
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
            "secret", "add", SECRET_NAME,
            "--provider", OPENAI_COMPATIBLE_PROVIDER,
            "--base-url", VALID_BASE_URL,
            "--model", MODEL,
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
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
          "secret", "add", SECRET_NAME,
          "--provider", "anthropic",
          "--model", MODEL,
          "--json",
        ],
        { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: "http://127.0.0.1:1/" } },
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
          "secret", "add", SECRET_NAME,
          "--provider", OPENAI_COMPATIBLE_PROVIDER,
          "--base-url", VALID_BASE_URL,
          "--api-key", API_KEY,
          "--json",
        ],
        { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: "http://127.0.0.1:1/" } },
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
          "secret", "add", SECRET_NAME,
          "--provider", "anthropic",
          "--base-url", VALID_BASE_URL,
          "--api-key", API_KEY,
          "--json",
        ],
        { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: "http://127.0.0.1:1/" } },
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
          "secret", "add", SECRET_NAME,
          "--model", MODEL,
          "--json",
        ],
        { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: "http://127.0.0.1:1/" } },
      );
      expect(code).not.toBe(0);
      const text = out.read() + err.read();
      // The clear typed-path pairing error (names --api-key / --provider / --base-url) …
      expect(text).toMatch(/--api-key|--provider|--base-url/i);
      // … NOT the generic "missing --data" hint the old fall-through produced.
      expect(text).not.toMatch(/missing --data/i);
    });
  });

  test("--data and the typed flags together are rejected (mutually exclusive)", async () => {
    await authedScope(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(
        [
          "secret", "add", SECRET_NAME,
          "--provider", OPENAI_COMPATIBLE_PROVIDER,
          "--base-url", VALID_BASE_URL,
          "--api-key", API_KEY,
          "--data", '{"x":1}',
          "--json",
        ],
        { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: "http://127.0.0.1:1/" } },
      );
      expect(code).not.toBe(0);
      const text = out.read() + err.read();
      expect(text).toMatch(/--data|both/i);
    });
  });
});
