import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FLEET_ID, GRANT_ID, WORKSPACE, response } from "@/tests/helpers/fleet-fixtures";
import { FleetListsProvider } from "../fleet/FleetListsProvider";
import { AccessRequest } from "./AccessRequest";

const GATE = {gate_id: "gate-access", workspace_id: WORKSPACE, fleet_id: FLEET_ID, status: "pending" as const, gate_type: "integration_grant",
  proposed_action: "mint short-lived credentials for github", tool_name: "request_access", action_name: "github", blast_radius: ""};
const GITHUB = {id: GRANT_ID, kind: "integration", name: "github", status: "pending", requested_scope: {repositories: ["org/app"], access: "write"}};
let grants: unknown[];
let reads: number;
beforeEach(() => {
  grants = [GITHUB]; reads = 0;
  vi.stubGlobal("fetch", vi.fn(async (path: string) => {
    if (path.endsWith("/integration-grants")) {reads += 1; return response({items: grants});}
    if (path.endsWith("/schedules")) return response({schedules: []});
    return response({items: [], total: 0, next_cursor: null});
  }));
});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

function card(evidence: Record<string, unknown> | undefined, lists = true) {
  const request = <AccessRequest gate={{...GATE, evidence}} />;
  return render(lists ? <FleetListsProvider workspace={WORKSPACE} fleet={FLEET_ID}>{request}</FleetListsProvider> : request);
}
const field = (term: string) => screen.getByText(term).nextElementSibling?.textContent;

it.each([["write", "Read and write"], ["read", "Read only"]] as const)("shows the repositories and %s reach a GitHub request asks for", async (reach, label) => {
  grants = [{...GITHUB, requested_scope: {repositories: ["org/app", "org/web"], access: reach}}];
  card({service: "github"});
  expect(await screen.findByText(label)).toBeTruthy();
  expect(field("Permissions")).toBe(label);
  expect(field("Access")).toBe("github");
  expect(field("Repositories")).toBe("org/app, org/web");
});

it("names a secret request alone, even with no lists to read", () => {
  card({service: "sentry"}, false);
  expect(screen.getByRole("definition").textContent).toBe("sentry");
  expect(screen.queryByText("Repositories")).toBeNull();
});

it("shows nothing for a card that names no service", () => {
  const view = card({image: "runner:release"});
  expect(view.container.textContent).toBe("");
});

it("reads the lists once more for a card raised after they were read, and no further", async () => {
  grants = [];
  const view = card({service: "github"});
  await waitFor(() => expect(reads).toBe(2));
  view.rerender(<FleetListsProvider workspace={WORKSPACE} fleet={FLEET_ID}><AccessRequest gate={{...GATE, evidence: {service: "github"}}} /></FleetListsProvider>);
  await new Promise(resolve => setTimeout(resolve, 20));
  expect(reads).toBe(2);
  expect(screen.queryByText("Repositories")).toBeNull();
  expect(field("Access")).toBe("github");
});
