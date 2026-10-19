import type { PlatformEntry } from "@/lib/api/platform-library-types";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import EditFleetDialog from "./EditFleetDialog";
import { LIBRARY_CARD } from "@/tests/helpers/draft-fixtures";

const {patch, refresh} = vi.hoisted(() => ({patch: vi.fn(), refresh: vi.fn()}));
vi.mock("../actions", () => ({patchPlatformLibraryAction: patch}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh})}));
const ENTRY: PlatformEntry = {...LIBRARY_CARD, visibility: "platform_draft", published: false, updated_at: "1970-01-01T00:00:00Z",
  requirements: {credentials: ["github"]}, required_credentials_reasons: {github: "Review code"}};
function show(entry = ENTRY) {
  const saved = vi.fn(), closed = vi.fn();
  return {...render(<EditFleetDialog entry={entry} open onOpenChange={closed} onSaved={saved} />), saved, closed};
}
beforeEach(() => vi.resetAllMocks());
afterEach(cleanup);

describe("native Library metadata editor", () => {
  it("holds dismissal during a save and permits cancellation once it settles", async () => {
    const pending = Promise.withResolvers<unknown>(); patch.mockReturnValue(pending.promise);
    const view = show({...ENTRY, required_credentials_reasons: {}});
    expect(screen.getByLabelText("github")).toHaveProperty("value", "");
    fireEvent.change(screen.getByLabelText("Name"), {target: {value: "Changed name"}});
    fireEvent.click(screen.getByRole("button", {name: "Save"}));
    fireEvent.keyDown(screen.getByRole("dialog"), {key: "Escape"});
    expect(view.closed).not.toHaveBeenCalled();
    await act(async () => pending.resolve({ok: false, error: "Refused", status: 409}));
    expect(screen.getByRole("alert").textContent).toContain("Refused");
    fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
    expect(view.closed).toHaveBeenCalledWith(false);
  });
  it("allows idle dismissal and accepts metadata without credential requirements", () => {
    const view = show({...ENTRY, requirements: {}});
    expect(screen.queryByLabelText("github")).toBeNull();
    fireEvent.click(screen.getByRole("button", {name: "Close"}));
    expect(view.closed).toHaveBeenCalledWith(false);
  });
  it("shows editable copy and credential explanations without source controls", () => {
    show(); expect(screen.getByLabelText("github")).toMatchObject({value: "Review code"});
    expect(screen.queryByLabelText("Repository")).toBeNull(); expect(screen.queryByLabelText("Ref")).toBeNull();
  });
  it("closes an unchanged editor without a mutation", async () => {
    const view = show(); fireEvent.click(screen.getByRole("button", {name: "Save"}));
    await waitFor(() => expect(view.closed).toHaveBeenCalledWith(false)); expect(patch).not.toHaveBeenCalled();
  });
  it("sends only changed fields with its opening tag even after props refresh", async () => {
    const view = show();
    view.rerender(<EditFleetDialog entry={{...ENTRY, name: "Other edit", etag: '\"newer\"'}} open onOpenChange={view.closed} onSaved={view.saved} />);
    fireEvent.change(screen.getByLabelText("Description"), {target: {value: "Clearer copy"}});
    patch.mockResolvedValue({ok: true, data: {...ENTRY, description: "Clearer copy"}});
    fireEvent.click(screen.getByRole("button", {name: "Save"}));
    await waitFor(() => expect(view.saved).toHaveBeenCalledWith(expect.objectContaining({description: "Clearer copy"})));
    expect(patch).toHaveBeenCalledWith(ENTRY.id, {description: "Clearer copy"}, ENTRY.etag);
  });
  it("preserves unsaved input and surfaces stale curation instead of replacing its tag", async () => {
    const view = show(); patch.mockResolvedValue({ok: false, status: 412, error: "Library changed"});
    fireEvent.change(screen.getByLabelText("Name"), {target: {value: "New label"}});
    fireEvent.click(screen.getByRole("button", {name: "Save"}));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("Library changed"));
    expect(screen.getByLabelText("Name")).toMatchObject({value: "New label"}); expect(view.saved).not.toHaveBeenCalled(); expect(refresh).toHaveBeenCalledOnce();
  });
  it("shows a failed action and retains the editor", async () => {
    const view = show(); patch.mockRejectedValue(new Error("Answer lost"));
    fireEvent.change(screen.getByLabelText("github"), {target: {value: "Read pull requests"}});
    fireEvent.click(screen.getByRole("button", {name: "Save"}));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("Try again"));
    expect(screen.getByRole("alert").textContent).not.toContain("Answer lost");
    expect(view.closed).not.toHaveBeenCalled(); expect(screen.getByLabelText("github")).toMatchObject({value: "Read pull requests"});
  });
  it("refuses an empty display name before writing", () => {
    show(); fireEvent.change(screen.getByLabelText("Name"), {target: {value: " "}});
    expect(screen.getByRole("button", {name: "Save"}).hasAttribute("disabled")).toBe(true); expect(patch).not.toHaveBeenCalled();
  });
});
