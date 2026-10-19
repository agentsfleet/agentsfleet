import { afterEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { IntentModuleSnapshot } from "@/components/domain/island-dynamic/intent-module-loader";
import { DRAFT, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { DraftDeleteDialogDynamic } from "./DraftDeleteDialogDynamic";

type DialogModule = typeof import("./DraftDeleteDialog");
const state = vi.hoisted(() => {
  const snapshot: IntentModuleSnapshot<DialogModule> = {error: null, module: null, status: "loading"};
  const importer: {load: (() => Promise<DialogModule>) | null} = {load: null};
  return {snapshot, importer, preload: vi.fn(), retry: vi.fn()};
});
vi.mock("@/components/domain/island-dynamic/intent-module-loader", () => ({
  INTENT_MODULE_STATUS: {error: "error"},
  createIntentModuleLoader: (load: () => Promise<DialogModule>) => {
    state.importer.load = load;
    return {preload: state.preload, retry: state.retry};
  },
  useIntentModule: () => state.snapshot,
}));
afterEach(cleanup);

it("announces loading, offers retry after a chunk failure, and opens the real confirmation", async () => {
  const props = {card: {id: DRAFT.id, title: DRAFT.title, detail: "Setup ready"}, workspace: WORKSPACE,
    remove: vi.fn(), onRemoved: vi.fn(), onClose: vi.fn()};
  const view = render(<DraftDeleteDialogDynamic {...props} />);
  expect(state.preload).toHaveBeenCalledOnce();
  expect(screen.getByRole("dialog").getAttribute("aria-busy")).toBe("true");
  state.snapshot = {error: new Error("Chunk unavailable"), module: null, status: "error"};
  view.rerender(<DraftDeleteDialogDynamic {...props} />);
  expect(screen.getByRole("alert").textContent).toBe("Could not load the chat controls.");
  fireEvent.click(screen.getByRole("button", {name: "Retry"}));
  expect(state.retry).toHaveBeenCalledOnce();
  state.snapshot = {error: null, module: await state.importer.load!(), status: "ready"};
  view.rerender(<DraftDeleteDialogDynamic {...props} />);
  expect(screen.getByRole("alertdialog").textContent).toContain(DRAFT.title);
  fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
  expect(props.onClose).toHaveBeenCalledOnce();
  expect(props.remove).not.toHaveBeenCalled();
});
