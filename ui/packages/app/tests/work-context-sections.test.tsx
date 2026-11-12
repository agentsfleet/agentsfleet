import type { FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { fleetSources } from "@/lib/fleet-sources";
import { WorkChanges, WorkOutputs, WorkSources, recordedChanges } from "@/components/domain/chat/WorkContextSections";
import { evt } from "./helpers/fleet-stream-fixtures";
import { detail, TASK_IDS } from "./helpers/task-reads";
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
  expect(screen.getByText("No sources configured.")).toBeTruthy();
  view.rerender(<WorkSources sources={fleetSources({repositories: ["a/b"]})} />);
  fireEvent.click(screen.getByRole("region", {name: "Sources"}).querySelector("summary")!);
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
  fireEvent.click(screen.getByText("2 files"));
  expect(screen.getByText("Write in progress")).toBeTruthy();
  expect(screen.getByText("Local write recorded")).toBeTruthy();
  expect(screen.queryByRole("link")).toBeNull();
  view.rerender(<WorkChanges events={[evt({tools: [tool({})]})]} />);
  expect(screen.getByText("1 file")).toBeTruthy();
});

it("related pull requests use saved target links without claiming they were published outputs", () => {
  const value = detail();
  const view = render(<WorkOutputs details={new Map([["task", {kind: "ready", value}]])} />);
  expect(screen.getByText("No outputs yet.")).toBeTruthy();
  const target = {kind: "existing_pr" as const, repository: "a/b", number: 123, branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: "https://github.com/a/b/pull/123", workflows: []};
  const saved = {...value, context: {...value.context, target}};
  view.rerender(<WorkOutputs details={new Map([["task", {kind: "ready", value: saved}], ["same-pr", {kind: "ready", value: saved}]])} />);
  expect(within(screen.getByRole("region", {name: "Pull requests"})).getByRole("link").getAttribute("href")).toBe(target.url);
  expect(screen.queryByRole("region", {name: "Outputs"})).toBeNull();
  expect(screen.getAllByRole("link")).toHaveLength(1);
  expect(screen.queryByText(/last confirmed/)).toBeNull();
  view.rerender(<WorkOutputs details={new Map([["task", {kind: "failed", reason: "unreachable", previous: saved}]])} />);
  expect(screen.getByText("a/b · last confirmed")).toBeTruthy();
  view.rerender(<WorkOutputs details={new Map([["task", {kind: "failed", reason: "access", previous: null}]])} />);
  expect(screen.queryByRole("link")).toBeNull();
});

it("lists a confirmed publication only as an output while retaining other related pull requests", () => {
  const task = detail();
  const target = {kind: "existing_pr" as const, repository: "a/b", number: 123, branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: "https://github.com/a/b/pull/123", workflows: []};
  const related = {...task, context: {...task.context, target}};
  const published = {...related, publication: {effect_id: TASK_IDS[0], ordinal: 1, state: "confirmed" as const,
    expected_head: target.head, tested_tree: target.tree, commit: "c".repeat(40), pull_request_url: target.url}};
  const other = {...related, context: {...related.context, target: {...target, number: 124, url: "https://github.com/a/b/pull/124"}}};
  render(<WorkOutputs details={new Map([
    ["published", {kind: "ready", value: published}], ["earlier", {kind: "ready", value: related}], ["other", {kind: "ready", value: other}],
  ])} />);
  expect(within(screen.getByRole("region", {name: "Outputs"})).getByRole("link", {name: "Pull request #123"})).toBeTruthy();
  expect(within(screen.getByRole("region", {name: "Pull requests"})).getByRole("link", {name: "Pull request #124"})).toBeTruthy();
  expect(screen.getAllByRole("link")).toHaveLength(2);
});


it("distinguishes outputs still loading from unavailable and confirmed empty targets", () => {
  const view = render(<WorkOutputs details={new Map()} expectedDetails={1} />);
  expect(screen.getByText("Loading outputs…")).toBeTruthy();
  view.rerender(<WorkOutputs details={new Map([["task", {kind: "loading", previous: null}]])} />);
  expect(screen.getByText("Loading outputs…")).toBeTruthy();
  view.rerender(<WorkOutputs details={new Map([["task", {kind: "failed", reason: "unreachable", previous: null}]])} />);
  expect(screen.getByText("Outputs unavailable.")).toBeTruthy();
  view.rerender(<WorkOutputs details={new Map([["task", {kind: "ready", value: detail()}]])} />);
  expect(screen.getByText("No outputs yet.")).toBeTruthy();
});

it("keeps multiple configured repositories behind one closed disclosure", () => {
  render(<WorkSources sources={fleetSources({repositories: ["a/b", "c/d"]})} />);
  const disclosure = screen.getByText("2 repositories").closest("summary");
  expect(disclosure?.parentElement?.getAttribute("open")).toBeNull();
  fireEvent.click(disclosure!);
  expect(screen.getByRole("link", {name: "c/d"}).getAttribute("href")).toBe("https://github.com/c/d");
});
