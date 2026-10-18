import { expect, test } from "bun:test";
import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse } from "./helpers-mock-api.ts";
import { catalogueModel, cataloguePage } from "./helpers-catalogue.ts";

test("models --advanced pages the inclusive catalogue without duplicating regular models", async () => {
  await withAuthedStateDir({ workspaceId: "ws_models_advanced", sessionId: "session_models_advanced" }, async () => {
    const paths: URL[] = [];
    const regular = catalogueModel("regular", "provider");
    const advanced = catalogueModel("advanced", "provider", { advanced: true });
    await withMockApi({ "GET /v1/models": (_request, url) => {
      paths.push(url);
      const special = url.searchParams.get("advanced") === "true";
      const next = url.searchParams.has("starting_after");
      return jsonResponse(200, special ? cataloguePage(next ? [advanced] : [regular], next ? null : "advanced-page-2")
        : cataloguePage([regular]));
    } }, async apiUrl => {
      const out = bufferStream(); const err = bufferStream();
      const code = await runCli(["models", "--advanced", "--provider", "provider", "--json"], {
        stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
      });
      expect(code).toBe(0);
      expect(JSON.parse(out.read()).models).toEqual([regular, advanced]);
      expect(paths.map(url => url.searchParams.get("advanced"))).toEqual(["true", "true"]);
      expect(paths.every(url => url.searchParams.get("provider") === "provider")).toBe(true);
      expect(paths[1]?.searchParams.get("starting_after")).toBe("advanced-page-2");
      expect(err.read()).not.toContain("error");
    });
  });
});
