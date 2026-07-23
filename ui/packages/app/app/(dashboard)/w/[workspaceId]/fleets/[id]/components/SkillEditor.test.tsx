import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SkillEditor from "./SkillEditor";
import type { FleetDetail } from "@/lib/types";
import { EVENTS } from "@/lib/analytics/events";
import {
  OUTCOME,
  SAVE_NEXT_WAKE_NOTICE,
  SAVE_STALE_RELOADED_NOTICE,
  SOURCE_FIELD,
  TRIGGER_DOC_EMPTY,
  VIEW_SOURCE_LABEL,
  type SourceField,
} from "./console-copy";

const HIDE_SOURCE_LABEL = "Hide source";
const SERVER_SOURCE_LABEL = "Current server version";
const UNSAVED_DRAFT_LABEL = "Your unsaved draft";

const saveFleetSourceAction = vi.fn();
const getFleetDetailAction = vi.fn();
const captureProductEvent = vi.fn();
const routerRefresh = vi.fn();

vi.mock("../../actions", () => ({
  saveFleetSourceAction: (...args: unknown[]) => saveFleetSourceAction(...args),
  getFleetDetailAction: (...args: unknown[]) => getFleetDetailAction(...args),
}));
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: (...args: unknown[]) => captureProductEvent(...args),
}));
vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh }) }));

function detail(over: Partial<FleetDetail> = {}): FleetDetail {
  return {
    id: "agt_1",
    name: "support-triage",
    status: "active",
    source_markdown: "# SKILL\noriginal",
    trigger_markdown: "# TRIGGER\noriginal",
    bundle_content_hash: null,
    triggers: null,
    events_processed: 0,
    budget_used_nanos: 0,
    created_at: 1,
    updated_at: 1,
    ...over,
  };
}

function renderEditor(field: SourceField = SOURCE_FIELD.skill) {
  return render(
    <SkillEditor
      workspaceId="ws_1"
      fleetId="agt_1"
      field={field}
      sourceMarkdown="# SKILL\noriginal"
      triggerMarkdown="# TRIGGER\noriginal"
      etag={'"seed"'}
    />,
  );
}

async function edit(field: SourceField, value: string) {
  renderEditor(field);
  const user = userEvent.setup({ delay: null });
  await user.click(screen.getByRole("button", { name: /Edit/ }));
  const label = field === SOURCE_FIELD.skill ? "Edit SKILL.md" : "Edit TRIGGER.md";
  fireEvent.change(screen.getByRole("textbox", { name: label }), { target: { value } });
  return user;
}

beforeEach(() => {
  saveFleetSourceAction.mockReset();
  getFleetDetailAction.mockReset();
  captureProductEvent.mockReset();
  routerRefresh.mockReset();
});
afterEach(() => cleanup());

describe("SkillEditor", () => {
  it("renders one visible document without internal tabs", () => {
    renderEditor();
    expect(screen.getByRole("button", { name: HIDE_SOURCE_LABEL })).toBeTruthy();
    expect(screen.queryByRole("tab")).toBeNull();
    expect(screen.getByLabelText("SKILL.md").textContent).toContain("original");
  });

  it("collapses and restores the selected skill document", async () => {
    renderEditor();
    await userEvent.click(screen.getByRole("button", { name: HIDE_SOURCE_LABEL }));
    expect(screen.queryByLabelText("SKILL.md")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: VIEW_SOURCE_LABEL }));
    expect(screen.getByLabelText("SKILL.md").textContent).toContain("original");
    expect(screen.queryByLabelText("TRIGGER.md")).toBeNull();
  });

  it("shows the empty trigger message only on the Trigger view", () => {
    render(
      <SkillEditor
        workspaceId="ws_1"
        fleetId="agt_1"
        field={SOURCE_FIELD.trigger}
        sourceMarkdown="# SKILL"
        triggerMarkdown={null}
        etag={'"seed"'}
      />,
    );
    expect(screen.getByText(TRIGGER_DOC_EMPTY)).toBeTruthy();
  });

  it.each([
    [SOURCE_FIELD.skill, "# SKILL\nedited", { source_markdown: "# SKILL\nedited" }],
    [SOURCE_FIELD.trigger, "# TRIGGER\nedited", { trigger_markdown: "# TRIGGER\nedited" }],
  ] as const)("saves only the selected %s document", async (field, value, body) => {
    saveFleetSourceAction.mockResolvedValue({
      ok: true,
      data: { etag: '"next"', config_revision: 2 },
    });
    const user = await edit(field, value);
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    expect(screen.getByText(SAVE_NEXT_WAKE_NOTICE)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(saveFleetSourceAction).toHaveBeenCalledWith(
      "ws_1",
      "agt_1",
      body,
      '"seed"',
    ));
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.fleet_source_saved, {
      fleet_id: "agt_1",
      field,
      outcome: OUTCOME.success,
    });
    expect(routerRefresh).toHaveBeenCalledTimes(1);
  });

  it("keeps the pending edit when a stale save reloads server source", async () => {
    saveFleetSourceAction.mockResolvedValue({ ok: false, status: 412, error: "stale" });
    getFleetDetailAction.mockResolvedValue({
      ok: true,
      data: { fleet: detail({ source_markdown: "# SKILL\nnew server" }), etag: '"fresh"' },
    });
    const user = await edit(SOURCE_FIELD.skill, "# SKILL\nmy draft");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.getByText(SAVE_STALE_RELOADED_NOTICE)).toBeTruthy());
    expect(screen.getByRole("textbox", { name: "Edit SKILL.md" })).toHaveProperty(
      "value",
      "# SKILL\nmy draft",
    );
    expect(screen.getByTestId(`source-comparison-${SERVER_SOURCE_LABEL.toLowerCase().replaceAll(" ", "-")}`).textContent).toContain(
      "# SKILL\nnew server",
    );
    expect(screen.getByTestId(`source-comparison-${UNSAVED_DRAFT_LABEL.toLowerCase().replaceAll(" ", "-")}`).textContent).toContain(
      "# SKILL\nmy draft",
    );
  });

  it("preserves a draft when refreshed props arrive for the same document", async () => {
    const view = renderEditor();
    const user = userEvent.setup({ delay: null });
    await user.click(screen.getByRole("button", { name: /Edit/ }));
    fireEvent.change(screen.getByRole("textbox", { name: "Edit SKILL.md" }), {
      target: { value: "# SKILL\nmy draft" },
    });
    view.rerender(
      <SkillEditor
        workspaceId="ws_1"
        fleetId="agt_1"
        field={SOURCE_FIELD.skill}
        sourceMarkdown="# SKILL\nserver refresh"
        triggerMarkdown="# TRIGGER"
        etag={'"fresh"'}
      />,
    );
    expect(screen.getByRole("textbox", { name: "Edit SKILL.md" })).toHaveProperty(
      "value",
      "# SKILL\nmy draft",
    );
  });

  it("resets editing when navigation changes the selected document", async () => {
    const view = renderEditor();
    await userEvent.click(screen.getByRole("button", { name: /Edit/ }));
    view.rerender(
      <SkillEditor
        workspaceId="ws_1"
        fleetId="agt_1"
        field={SOURCE_FIELD.trigger}
        sourceMarkdown="# SKILL\noriginal"
        triggerMarkdown="# TRIGGER\nserver"
        etag={'"fresh"'}
      />,
    );
    await waitFor(() => expect(screen.queryByRole("textbox")).toBeNull());
    expect(screen.getByRole("button", { name: VIEW_SOURCE_LABEL })).toBeTruthy();
  });

  it("places the edit cursor at the beginning of a full-height source editor", async () => {
    render(
      <SkillEditor
        workspaceId="ws_1"
        fleetId="agt_1"
        field={SOURCE_FIELD.trigger}
        sourceMarkdown="# SKILL"
        triggerMarkdown="# TRIGGER\noriginal"
        etag={'"seed"'}
        fillAvailableSpace
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: /Edit/ }));

    const editor = screen.getByRole("textbox", { name: "Edit TRIGGER.md" });
    expect(document.activeElement).toBe(editor);
    expect(editor).toHaveProperty("selectionStart", 0);
    expect(editor.className).toContain("min-h-96");
    expect(editor.className).toContain("flex-1");
  });

  it("keeps the cursor where the operator places it after refocusing", async () => {
    render(
      <SkillEditor
        workspaceId="ws_1"
        fleetId="agt_1"
        field={SOURCE_FIELD.trigger}
        sourceMarkdown="# SKILL"
        triggerMarkdown="# TRIGGER\noriginal"
        etag={'"seed"'}
        fillAvailableSpace
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: /Edit/ }));

    const editor = screen.getByRole("textbox", { name: "Edit TRIGGER.md" }) as HTMLTextAreaElement;
    editor.setSelectionRange(10, 10);
    fireEvent.focus(editor);

    expect(editor.selectionStart).toBe(10);
  });

  it("cancels an edit and restores the durable document", async () => {
    await edit(SOURCE_FIELD.skill, "# SKILL\nthrow this away");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByLabelText("SKILL.md").textContent).toContain("original");
  });

  it("surfaces a failure while reloading after a stale save", async () => {
    saveFleetSourceAction.mockResolvedValue({ ok: false, status: 412, error: "stale" });
    getFleetDetailAction.mockResolvedValue({
      ok: false,
      status: 503,
      error: "Source reload unavailable",
      errorCode: "UZ-AGT-503",
    });
    const user = await edit(SOURCE_FIELD.skill, "# SKILL\nmy draft");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.getByText(/Source reload unavailable/i)).toBeTruthy());
  });

  it("surfaces a save failure and records only coarse analytics", async () => {
    saveFleetSourceAction.mockResolvedValue({
      ok: false,
      status: 500,
      error: "storage refused",
      errorCode: "UZ-AGT-500",
    });
    const user = await edit(SOURCE_FIELD.skill, "# SKILL\nedited");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.getByText(/storage refused/i)).toBeTruthy());
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.fleet_source_saved, {
      fleet_id: "agt_1",
      field: SOURCE_FIELD.skill,
      outcome: OUTCOME.failure,
    });
  });
});
