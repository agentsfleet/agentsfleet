import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { withTooltipProvider, getModelLibraryActionMock, entry, registry, LIBRARY, renderTableWithLibrary } from "./helpers/models-registry-fixtures";

describe("ModelsRegistryTable — hover speculation on a row's Edit control", () => {
  // happy-dom answers `(pointer: coarse)` affirmatively, which is the correct
  // answer for a headless document and the wrong one for the desktop pointer
  // this policy is written for. Choosing the answer is what puts the fine-
  // pointer arm — the one every mouse user takes — under test at all.
  function stubFinePointer() {
    vi.stubGlobal(
      "matchMedia",
      vi.fn((q: string) => ({ matches: false, media: q })),
    );
  }

  afterEach(() => vi.unstubAllGlobals());

  /** Mounts inside a real provider and leaves the catalogue IDLE — unlike
   *  `renderTableWithLibrary`, which focuses an Edit control to warm it. Hover
   *  policy is only observable from a cold start. */
  async function renderTableColdCatalogue() {
    getModelLibraryActionMock.mockResolvedValue({ ok: true, data: LIBRARY });
    const { ModelCatalogueProvider } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider"
    );
    const { default: ModelsRegistryTable } = await import(
      "../app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable"
    );
    render(
      withTooltipProvider(
        React.createElement(
          ModelCatalogueProvider,
          null,
          React.createElement(ModelsRegistryTable, {
            workspaceId: "ws_1",
            initialPage: { ...registry([entry({ id: "e1" })]), next_cursor: null, total: null },
            initialError: null,
          } as never),
        ),
      ),
    );
    expect(getModelLibraryActionMock).not.toHaveBeenCalled();
  }

  it("warms the catalogue when hover is a real signal", async () => {
    // The Edit dialog's picker needs the global catalogue. Warming it on hover
    // means the dialog opens populated rather than filling in underneath the
    // user's cursor — and an ordinary Models visit still pays nothing, because
    // nothing fetches until the gesture.
    stubFinePointer();
    await renderTableColdCatalogue();

    const user = userEvent.setup();
    await user.hover(screen.getByRole("button", { name: "Edit claude-sonnet-5" }));

    await waitFor(() => expect(getModelLibraryActionMock).toHaveBeenCalledTimes(1));
  });

  it("warms the catalogue from the Create-model trigger on the same policy", async () => {
    // The Add dialog carries its own copy of the hover gate, because its
    // trigger lives in a different component from the row controls. Both must
    // agree, or one of the two ways into the picker opens cold.
    stubFinePointer();
    await renderTableColdCatalogue();

    const user = userEvent.setup();
    await user.hover(screen.getByRole("button", { name: /create model/i }));

    await waitFor(() => expect(getModelLibraryActionMock).toHaveBeenCalledTimes(1));
  });

  it("does not speculate on a coarse pointer, where hover is already a press", async () => {
    vi.stubGlobal(
      "matchMedia",
      vi.fn((q: string) => ({ matches: q.includes("coarse"), media: q })),
    );
    await renderTableColdCatalogue();

    const user = userEvent.setup();
    await user.hover(screen.getByRole("button", { name: "Edit claude-sonnet-5" }));

    expect(getModelLibraryActionMock).not.toHaveBeenCalled();
  });
});
