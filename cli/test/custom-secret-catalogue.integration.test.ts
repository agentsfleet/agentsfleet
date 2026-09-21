// `secret create` against the live provider catalogue — a closed set when it
// can be read, and deliberately open when it cannot. An unreachable or empty
// catalogue must not block a write, or a fresh environment is unusable.

import { describe, test, expect } from "bun:test";
import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";
import { OPENAI_COMPATIBLE_PROVIDER, SECRET_FIELD_PROVIDER } from "../src/constants/custom-endpoint.ts";
import {
  CATALOGUE_PAGE,
  WS_ID,
  SECRET_NAME,
  API_KEY,
  MODEL,
  CATALOGUE_MODEL,
  authedScope,
} from "./helpers-custom-secret.ts";

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

  test("a model the provider does not serve is refused, and never POSTed", async () => {
    await authedScope(async () => {
      // The twin of the --provider hole: --model was checked nowhere, so a
      // typo stored a credential that reported success and failed at the first
      // event. One catalogue read now closes both.
      const routes: MockRoutes = {
        "GET /v1/models": () => jsonResponse(200, CATALOGUE_PAGE),
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
            "--model", "claude-opus-4",
            "--json",
          ],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).not.toBe(0);
        expect(calls.some((c) => c.method === "POST")).toBe(false);
        const text = out.read() + err.read();
        expect(text).toContain("claude-opus-4");
        // Scoped to anthropic — openai's models are not offered as fixes for
        // an anthropic credential.
        expect(text).toContain(CATALOGUE_MODEL);
        expect(text).not.toContain("gpt-5.6-sol");
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
            "--model", CATALOGUE_MODEL,
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
