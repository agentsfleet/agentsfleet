import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DRAFT_LIST_STATUS } from "@/lib/api/draft-list-types";
import { DRAFT, FLEET, MESSAGE, MODEL, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { SetupStatusFlap } from "./SetupStatusFlap";

afterEach(cleanup);

const SAVED = {...EMPTY_DRAFT_VIEW, fleet: FLEET};

it.each([
  EMPTY_DRAFT_VIEW,
  {...SAVED, inaccessible: true},
  {...EMPTY_DRAFT_VIEW, preview: {definition: DRAFT.review!.effective.definition, instanceName: "repairer", revision: "1", setupRevision: "1"}},
  {...EMPTY_DRAFT_VIEW, draft: {...DRAFT, candidate: undefined}},
])("does not invent a saved result for an unavailable or unconfirmed setup %#", view => {
  const {container} = render(<SetupStatusFlap view={view} workspace={WORKSPACE} />);
  expect(container.textContent).toBe("");
});

it.each(["reading", "sending", "saving"] as const)("shows %s only while that operation is recorded", setupAction => {
  const view = {...SAVED, setupAction, busy: true};
  render(<SetupStatusFlap view={view} workspace={WORKSPACE} />);
  const status = screen.getByRole("status", {name: "Setup status"});
  expect(status.textContent).toContain({reading: "Reading setup", sending: "Sending request", saving: "Saving"}[setupAction]);
  expect(status.getAttribute("data-setup-status")).toBe("working");
  expect(status.querySelectorAll('svg[aria-hidden="true"]').length).toBeGreaterThan(0);
  expect(screen.queryByRole("link")).toBeNull();
});

it.each([
  {patch: {saveOperation: "accepted-save"}, label: "Saving", animated: true},
  {patch: {optimistic: {operation: "pending", text: "Refine", createdAt: MESSAGE.created_at}}, label: "Sending request", animated: true},
  {patch: {messages: [{...MESSAGE, state: "queued" as const}]}, label: "Waiting to refine setup", animated: true},
  {patch: {messages: [{...MESSAGE, state: "running" as const}]}, label: "Refining setup", animated: true},
])("preserves the actual pending state: $label", ({patch, label, animated}) => {
  render(<SetupStatusFlap view={{...EMPTY_DRAFT_VIEW, draft: DRAFT, ...patch}} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toContain(label);
  expect(screen.getByText(label).getAttribute("data-chat-loading")).toBe(String(animated));
  expect(screen.getByRole("status").getAttribute("data-setup-status")).toBe("working");
  expect(screen.queryByRole("link")).toBeNull();
});

it.each([false, true])("keeps a failed or unconfirmed operation separate from saved success (%s)", pending => {
  render(<SetupStatusFlap view={{...SAVED, error: "A request failed", pending, setupAction: "saving"}} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toBe(pending ? "Update not confirmed" : "Setup needs attention");
  expect(screen.getByRole("status").getAttribute("data-setup-status")).toBe("attention");
  expect(screen.queryByRole("link")).toBeNull();
});

it.each([false, true])("marks manual and proposed changes as unsaved (%s)", dirty => {
  render(<SetupStatusFlap view={{...SAVED, draft: dirty ? null : DRAFT}} workspace={WORKSPACE} dirty={dirty} />);
  expect(screen.getByRole("status").textContent).toBe("Unsaved setup changes");
  expect(screen.getByRole("status").getAttribute("data-setup-status")).toBe("attention");
  expect(screen.queryByRole("link")).toBeNull();
});

it("makes an unsaved proposal available for saving without a success tick", () => {
  render(<SetupStatusFlap view={{...EMPTY_DRAFT_VIEW, draft: DRAFT}} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toBe("Setup ready to save");
  expect(screen.getByRole("status").getAttribute("data-setup-status")).toBe("attention");
});

it("does not keep refining after a stopped request", () => {
  render(<SetupStatusFlap view={{...EMPTY_DRAFT_VIEW, draft: DRAFT, messages: [{...MESSAGE, state: "cancelled"}]}} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toBe("Setup ready to save");
});

it("offers Open fleet for an accepted version without claiming activation", () => {
  render(<SetupStatusFlap view={SAVED} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toContain("Setup saved · v1");
  expect(screen.getByRole("status").textContent).not.toContain("ready");
  expect(screen.getByRole("link", {name: "Open fleet"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/fleets/${FLEET.id}`);
});

it("opens the refreshed fleet view when setup is already on that page", () => {
  const open = vi.fn();
  render(<SetupStatusFlap view={SAVED} workspace={WORKSPACE} onOpenFleet={open} />);
  fireEvent.click(screen.getByRole("button", {name: "Open fleet"}));
  expect(open).toHaveBeenCalledOnce();
  expect(screen.queryByRole("link", {name: "Open fleet"})).toBeNull();
});

it.each([
  {kind: "setup_ready", revision: "1"},
  {kind: "setup_saved", revision: "2", origin: "cli"},
  {kind: "unsupported", revision: "1"},
  {kind: "setup_saved", revision: "1"},
  {kind: "setup_saved", revision: "1", origin: "cli"},
])("uses only a matching saved receipt for provenance %#", payload => {
  const activities = [{turn_id: "receipt", event_type: "activity", status: "recorded", request_json: JSON.stringify(payload), created_at: MESSAGE.created_at}];
  render(<SetupStatusFlap view={{...SAVED, activities}} workspace={WORKSPACE} />);
  const cli = payload.kind === "setup_saved" && payload.revision === "1" && payload.origin === "cli";
  expect(screen.getByRole("status").textContent).toContain(cli ? "Setup saved via CLI · v1" : "Setup saved · v1");
});

const READY = {...EMPTY_DRAFT_VIEW, listStatus: DRAFT_LIST_STATUS.ready};

it.each([DRAFT_LIST_STATUS.loading, DRAFT_LIST_STATUS.unavailable])("does not claim there are no models while the list is %s", listStatus => {
  const {container} = render(<SetupStatusFlap view={{...EMPTY_DRAFT_VIEW, listStatus}} workspace={WORKSPACE} modelsHref={`/w/${WORKSPACE}/settings/models`} />);
  expect(container.textContent).toBe("");
});

it("stays quiet when a model can author", () => {
  const {container} = render(<SetupStatusFlap view={{...READY, models: [MODEL]}} workspace={WORKSPACE} modelsHref={`/w/${WORKSPACE}/settings/models`} />);
  expect(container.textContent).toBe("");
});

it.each([
  {name: "no model is configured", patch: {}, label: "No chat model yet"},
  {name: "no configured model can author", patch: {models: [{...MODEL, eligibility: {setup: false, runtime: true}}]}, label: "No chat model available"},
])("sends the owner to model settings when $name", ({patch, label}) => {
  render(<SetupStatusFlap view={{...READY, ...patch}} workspace={WORKSPACE} modelsHref={`/w/${WORKSPACE}/settings/models`} />);
  expect(screen.getByRole("status", {name: "Setup status"}).textContent).toBe(`${label}Set up model`);
  expect(screen.getByRole("status").getAttribute("data-setup-status")).toBe("attention");
  expect(screen.getByRole("link", {name: "Set up model"}).getAttribute("href")).toBe(`/w/${WORKSPACE}/settings/models`);
});

it("names the missing model without a link when the caller gives no settings page", () => {
  render(<SetupStatusFlap view={READY} workspace={WORKSPACE} />);
  expect(screen.getByRole("status").textContent).toBe("No chat model yet");
  expect(screen.queryByRole("link")).toBeNull();
});

it("asks for more models, with no link, while the model search is incomplete", () => {
  render(<SetupStatusFlap view={{...READY, moreModels: true}} workspace={WORKSPACE} modelsHref={`/w/${WORKSPACE}/settings/models`} />);
  expect(screen.getByRole("status").textContent).toBe("Load more models to find one for chat");
  expect(screen.queryByRole("link")).toBeNull();
});

it("lets an in-flight save or a failure speak before the missing model", () => {
  const {rerender} = render(<SetupStatusFlap view={{...READY, saveOperation: "accepted-save"}} workspace={WORKSPACE} modelsHref={`/w/${WORKSPACE}/settings/models`} />);
  expect(screen.getByRole("status").textContent).toContain("Saving");
  rerender(<SetupStatusFlap view={{...READY, error: "A request failed"}} workspace={WORKSPACE} modelsHref={`/w/${WORKSPACE}/settings/models`} />);
  expect(screen.getByRole("status").textContent).toBe("Setup needs attention");
});
