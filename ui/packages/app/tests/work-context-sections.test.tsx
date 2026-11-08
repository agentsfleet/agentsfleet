import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { fleetSources } from "@/lib/fleet-sources";
import { WorkChanges, WorkOutputs, WorkSources, recordedChanges } from "@/components/domain/chat/WorkContextSections";
import { evt } from "./helpers/fleet-stream-fixtures";
import { detail } from "./helpers/task-reads";
afterEach(cleanup);

it("projects only explicitly saved repository policy and rejects malformed repository links", () => {
  for (const config of [null, [], "bad", {}, {repositories: true}]) expect(fleetSources(config)).toEqual({repositories: [], access: null, publication: null});
  const saved = fleetSources({repositories: ["agentsfleet/agentsfleet", "agentsfleet/agentsfleet", "a/b", "../bad", "a/..", "https://evil.test", "a/b/c", 9],
    repository_access: "write", repository_publication: "existing_pr", installation_token: "secret", webhook_ingress: ["other/repo"]});
  expect(saved).toEqual({repositories: ["agentsfleet/agentsfleet", "a/b"], access: "write", publication: "existing_pr"});
  expect(fleetSources({repository_access: "read", repository_publication: "event_draft"})).toEqual({repositories: [], access: "read", publication: "event_draft"});
  expect(fleetSources({repository_access: "admin", repository_publication: "push"})).toEqual({repositories: [], access: null, publication: null});
});

it("labels sources as saved policy without claiming fresh connected permissions", () => {
  const view = render(<WorkSources />);
  expect(screen.getByText("No saved repository scope.")).toBeTruthy();
  view.rerender(<WorkSources sources={fleetSources({repositories: ["a/b"]})} />);
  expect(screen.getByRole("link", {name: "a/b"}).getAttribute("href")).toBe("https://github.com/a/b");
  expect(screen.getByText("Saved fleet policy · access not specified")).toBeTruthy();
  for (const publication of ["existing_pr", "event_draft"]) {
    view.rerender(<WorkSources sources={fleetSources({repositories: ["a/b"], repository_access: "write", repository_publication: publication})} />);
    expect(screen.getByText(publication === "existing_pr" ? "Saved fleet policy · write access · existing PRs" : "Saved fleet policy · write access · draft PRs")).toBeTruthy();
  }
});

function tool(over: Partial<FleetToolCall>): FleetToolCall {
  return {name: "file_write", args: {path: "src/repair.ts"}, startedAtMs: 0, ms: 1, done: true, status: "succeeded", ...over};
}
it("changes contain recorded writes, exclude reads and failed or unconfirmed mutations, and never render a diff", () => {
  const ignored = [tool({name: "file_read"}), tool({status: "failed"}), tool({status: "interrupted"}), tool({status: undefined}), tool({args: {path: 42}}), tool({args: {path: " "}}), tool({args: undefined})];
  expect(recordedChanges([evt(), evt({tools: ignored})])).toEqual([]);
  const events = [evt({tools: [tool({}), tool({name: "file_edit", args: {path: "src/test.ts"}, done: false, status: undefined}), tool({})]})];
  expect(recordedChanges(events)).toEqual([{path: "src/repair.ts", running: false}, {path: "src/test.ts", running: true}]);
  const view = render(<WorkChanges events={[]} />);
  expect(screen.queryByRole("region", {name: "Changes"})).toBeNull();
  view.rerender(<WorkChanges events={events} />);
  fireEvent.click(screen.getByText("2 recorded files"));
  expect(screen.getByText("Write in progress")).toBeTruthy();
  expect(screen.getByText("Local write recorded")).toBeTruthy();
  expect(screen.queryByRole("link")).toBeNull();
  view.rerender(<WorkChanges events={[evt({tools: [tool({})]})]} />);
  expect(screen.getByText("1 recorded file")).toBeTruthy();
});

it("outputs use saved native target links, never objective prose or local edits as publication proof", () => {
  const value = detail();
  const view = render(<WorkOutputs details={new Map([["task", {kind: "ready", value}]])} />);
  expect(screen.queryByRole("region", {name: "Outputs"})).toBeNull();
  const target = {kind: "existing_pr" as const, repository: "a/b", number: 123, branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: "https://github.com/a/b/pull/123", workflows: []};
  const saved = {...value, context: {...value.context, target}};
  view.rerender(<WorkOutputs details={new Map([["task", {kind: "ready", value: saved}], ["same-pr", {kind: "ready", value: saved}]])} />);
  expect(within(screen.getByRole("region", {name: "Outputs"})).getByRole("link").getAttribute("href")).toBe(target.url);
  expect(screen.getAllByRole("link")).toHaveLength(1);
  expect(screen.queryByText(/last confirmed/)).toBeNull();
  view.rerender(<WorkOutputs details={new Map([["task", {kind: "failed", reason: "unreachable", previous: saved}]])} />);
  expect(screen.getByText("a/b · last confirmed")).toBeTruthy();
  view.rerender(<WorkOutputs details={new Map([["task", {kind: "failed", reason: "access", previous: null}]])} />);
  expect(screen.queryByRole("link")).toBeNull();
});
