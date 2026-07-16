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
  TRIGGER_DOC_LABEL,
} from "./console-copy";

const saveFleetSourceAction = vi.fn();
const getFleetDetailAction = vi.fn();
const captureProductEvent = vi.fn();
const routerRefresh = vi.fn();

vi.mock("../../actions", () => ({
  saveFleetSourceAction: (...a: unknown[]) => saveFleetSourceAction(...a),
  getFleetDetailAction: (...a: unknown[]) => getFleetDetailAction(...a),
}));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: (...a: unknown[]) => captureProductEvent(...a) }));
vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: routerRefresh }) }));

function detail(over: Partial<FleetDetail> = {}): FleetDetail {
  return {
    id: "agt_1",
    name: "platform-ops",
    status: "active",
    source_markdown: "# SKILL\noriginal",
    trigger_markdown: null,
    bundle_content_hash: null,
    triggers: null,
    events_processed: 0,
    budget_used_nanos: 0,
    created_at: 1,
    updated_at: 1,
    ...over,
  };
}

function renderEditor() {
  return render(
    <SkillEditor workspaceId="ws_1" fleetId="agt_1" sourceMarkdown="# SKILL\noriginal" triggerMarkdown={null} etag='"seed"' />,
  );
}

async function enterEditAndType(value: string) {
  renderEditor();
  const user = userEvent.setup({ delay: null });
  await user.click(screen.getByRole("button", { name: /Edit/ }));
  fireEvent.change(screen.getByRole("textbox", { name: "Edit SKILL.md" }), { target: { value } });
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
  it("test_source_save_next_wake_semantics", async () => {
    saveFleetSourceAction.mockResolvedValue({ ok: true, data: { etag: '"next"', config_revision: 2 } });
    const user = await enterEditAndType("# SKILL\nedited");

    await user.click(screen.getByRole("button", { name: "Save changes" }));
    // The dialog states the exact next-wake contract.
    expect(screen.getByText(SAVE_NEXT_WAKE_NOTICE)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(saveFleetSourceAction).toHaveBeenCalledWith("ws_1", "agt_1", { source_markdown: "# SKILL\nedited" }, '"seed"'),
    );
    // No reload / re-provision call on the happy path.
    expect(getFleetDetailAction).not.toHaveBeenCalled();
  });

  it("keeps the confirm pending until the source save finishes", async () => {
    let finish!: (value: unknown) => void;
    saveFleetSourceAction.mockReturnValueOnce(new Promise((resolve) => { finish = resolve; }));
    const user = await enterEditAndType("# SKILL\nedited");
    await user.click(screen.getByRole("button", { name: "Save changes" }));

    const confirm = screen.getByRole("button", { name: "Save" });
    await user.click(confirm);
    expect(confirm).toHaveProperty("disabled", true);
    await user.click(confirm);
    expect(saveFleetSourceAction).toHaveBeenCalledTimes(1);

    finish({ ok: true, data: { etag: '"next"', config_revision: 2 } });
    await waitFor(() => expect(routerRefresh).toHaveBeenCalledTimes(1));
  });

  it("preserves an unsaved sibling draft after saving the active document", async () => {
    saveFleetSourceAction.mockResolvedValue({ ok: true, data: { etag: '"next"', config_revision: 2 } });
    renderEditor();
    const user = userEvent.setup({ delay: null });
    await user.click(screen.getByRole("button", { name: /Edit/ }));
    fireEvent.change(screen.getByRole("textbox", { name: "Edit SKILL.md" }), {
      target: { value: "# SKILL\nsaved edit" },
    });
    await user.click(screen.getByRole("tab", { name: TRIGGER_DOC_LABEL }));
    fireEvent.change(screen.getByRole("textbox", { name: "Edit TRIGGER.md" }), {
      target: { value: "# TRIGGER\nsaved edit" },
    });
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(screen.getByRole("textbox", { name: "Edit SKILL.md" })).toHaveProperty(
      "value",
      "# SKILL\nsaved edit",
    ));
  });

  it("preserves the active draft when refreshed props arrive during editing", async () => {
    const view = renderEditor();
    const user = userEvent.setup({ delay: null });
    await user.click(screen.getByRole("button", { name: /Edit/ }));
    fireEvent.change(screen.getByRole("textbox", { name: "Edit SKILL.md" }), {
      target: { value: "# SKILL\nactive draft" },
    });

    view.rerender(
      <SkillEditor
        workspaceId="ws_1"
        fleetId="agt_1"
        sourceMarkdown={"# SKILL\nserver refresh"}
        triggerMarkdown={"# TRIGGER\nserver refresh"}
        etag='"fresh"'
      />,
    );
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Edit SKILL.md" })).toHaveProperty(
      "value",
      "# SKILL\nactive draft",
    ));
    await user.click(screen.getByRole("tab", { name: TRIGGER_DOC_LABEL }));
    expect(screen.getByRole("textbox", { name: "Edit TRIGGER.md" })).toHaveProperty(
      "value",
      "# TRIGGER\nserver refresh",
    );
  });

  it("adopts refreshed source props and their ETag while idle", async () => {
    saveFleetSourceAction.mockResolvedValue({ ok: true, data: { etag: '"next"', config_revision: 3 } });
    const view = renderEditor();
    view.rerender(
      <SkillEditor
        workspaceId="ws_1"
        fleetId="agt_1"
        sourceMarkdown={"# SKILL\nserver refresh"}
        triggerMarkdown={null}
        etag='"fresh"'
      />,
    );
    const user = userEvent.setup({ delay: null });
    await waitFor(() => expect(
      screen.getAllByLabelText("SKILL.md").find((node) => node.tagName === "PRE")?.textContent,
    ).toBe("# SKILL\nserver refresh"));
    await user.click(screen.getByRole("button", { name: /Edit/ }));
    fireEvent.change(screen.getByRole("textbox", { name: "Edit SKILL.md" }), {
      target: { value: "# SKILL\nafter refresh" },
    });
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveFleetSourceAction).toHaveBeenCalledWith(
      "ws_1",
      "agt_1",
      { source_markdown: "# SKILL\nafter refresh" },
      '"fresh"',
    ));
  });

  it("test_source_diff_panel_shows_pending_change", async () => {
    renderEditor();
    const user = userEvent.setup({ delay: null });
    await user.click(screen.getByRole("button", { name: /Edit/ }));
    // Unchanged source → no diff panel.
    expect(screen.queryByTestId("source-diff")).toBeNull();
    // An edit → the diff panel shows the pending change.
    fireEvent.change(screen.getByRole("textbox", { name: "Edit SKILL.md" }), { target: { value: "# SKILL\noriginal\nadded line" } });
    const diff = screen.getByTestId("source-diff");
    expect(diff.textContent).toContain("added line");
  });

  it("Cancel exits edit mode and restores the original source", async () => {
    const user = await enterEditAndType("# SKILL\nthrowaway");
    expect(screen.getByTestId("source-diff").textContent).toContain("throwaway");

    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(screen.queryByTestId("source-diff")).toBeNull();
    expect(screen.queryByRole("textbox", { name: "Edit SKILL.md" })).toBeNull();
    const readOnlySource = screen.getAllByLabelText("SKILL.md").find((node) => node.tagName === "PRE");
    expect(readOnlySource?.textContent).toContain("# SKILL");
    expect(readOnlySource?.textContent).toContain("original");
  });

  it("renders the trigger empty hint when no TRIGGER.md exists", async () => {
    renderEditor();
    const user = userEvent.setup({ delay: null });
    await user.click(screen.getByRole("tab", { name: TRIGGER_DOC_LABEL }));
    expect(screen.getByText(TRIGGER_DOC_EMPTY)).toBeTruthy();
  });

  it("test_source_save_emits_event", async () => {
    saveFleetSourceAction.mockResolvedValue({ ok: true, data: { etag: '"next"', config_revision: 2 } });
    const user = await enterEditAndType("# SKILL\nedited");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.fleet_source_saved, {
        fleet_id: "agt_1",
        field: SOURCE_FIELD.skill,
        outcome: OUTCOME.success,
      }),
    );
    // Privacy: fleet id + field + outcome only — never the source markdown.
    const props = captureProductEvent.mock.calls[0]?.[1] ?? {};
    expect(Object.keys(props).sort()).toEqual(["field", "fleet_id", "outcome"]);
  });

  it("surfaces a save failure and records the failed outcome", async () => {
    saveFleetSourceAction.mockResolvedValue({ ok: false, status: 500, error: "storage refused", errorCode: "UZ-AGT-500" });
    const user = await enterEditAndType("# SKILL\nedited");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(screen.getByText(/Couldn't save the source/)).toBeTruthy());
    expect(captureProductEvent).toHaveBeenCalledWith(EVENTS.fleet_source_saved, {
      fleet_id: "agt_1",
      field: SOURCE_FIELD.skill,
      outcome: OUTCOME.failure,
    });
    expect(routerRefresh).not.toHaveBeenCalled();
  });

  it("a stale If-Match (412) reloads the current source and re-diffs — never a silent overwrite", async () => {
    saveFleetSourceAction.mockResolvedValue({ ok: false, status: 412, error: "stale", errorCode: "UZ-AGT-014" });
    getFleetDetailAction.mockResolvedValue({
      ok: true,
      data: { fleet: detail({ source_markdown: "# SKILL\nsomeone else changed this" }), etag: '"fresh"' },
    });
    const user = await enterEditAndType("# SKILL\nmy edit");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(getFleetDetailAction).toHaveBeenCalledWith("ws_1", "agt_1"));
    // The dialog reloaded-and-rediffed against the fresh source, not overwrote it.
    await waitFor(() => expect(screen.getByText(SAVE_STALE_RELOADED_NOTICE)).toBeTruthy());
    // The operator's draft survives for a re-save against the fresh ETag.
    expect(screen.getByTestId("source-diff").textContent).toContain("my edit");
  });

  it("drops a stale sibling draft when a conflict reloads both documents", async () => {
    saveFleetSourceAction.mockResolvedValue({ ok: false, status: 412, error: "stale", errorCode: "UZ-AGT-014" });
    getFleetDetailAction.mockResolvedValue({
      ok: true,
      data: {
        fleet: detail({
          source_markdown: "# SKILL\nserver edit",
          trigger_markdown: "# TRIGGER\nserver trigger",
        }),
        etag: '"fresh"',
      },
    });
    renderEditor();
    const user = userEvent.setup({ delay: null });
    await user.click(screen.getByRole("button", { name: /Edit/ }));
    fireEvent.change(screen.getByRole("textbox", { name: "Edit SKILL.md" }), {
      target: { value: "# SKILL\nstale sibling draft" },
    });
    await user.click(screen.getByRole("tab", { name: TRIGGER_DOC_LABEL }));
    fireEvent.change(screen.getByRole("textbox", { name: "Edit TRIGGER.md" }), {
      target: { value: "# TRIGGER\nmy active edit" },
    });
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(screen.getByText(SAVE_STALE_RELOADED_NOTICE)).toBeTruthy());
    await user.click(screen.getByRole("tab", { name: "SKILL.md" }));
    expect(screen.getByRole("textbox", { name: "Edit SKILL.md" })).toHaveProperty(
      "value",
      "# SKILL\nserver edit",
    );
  });

  it("surfaces a stale-save reload failure and keeps the editor open", async () => {
    saveFleetSourceAction.mockResolvedValue({ ok: false, status: 412, error: "stale", errorCode: "UZ-AGT-014" });
    getFleetDetailAction.mockResolvedValue({ ok: false, status: 500, error: "reload failed", errorCode: "UZ-AGT-500" });
    const user = await enterEditAndType("# SKILL\nmy edit");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(screen.getByText(/Couldn't reload the source/)).toBeTruthy());
    expect(screen.getByRole("textbox", { name: "Edit SKILL.md" })).toBeTruthy();
    expect(routerRefresh).not.toHaveBeenCalled();
  });
});
