import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CATALOGUE_STATUS, type CatalogueStatus } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/catalogue-status";
import { SECRETS_LOAD } from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/secrets-load";
import type { LibraryModel } from "@/lib/api/model-library-types";

const { catalogue } = vi.hoisted(() => ({
  catalogue: { status: "loading" as CatalogueStatus, models: [] as LibraryModel[], preload: vi.fn() },
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider", () => ({
  useModelCatalogue: () => catalogue,
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/settings/models/actions", () => ({
  createModelEntryAction: vi.fn(), replaceSecretAction: vi.fn(), setProviderSelfManagedAction: vi.fn(),
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/secrets/actions", () => ({ createSecretAction: vi.fn() }));

import AddModelEntryDialog from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog";

const props = {
  workspaceId: "ws-loading", secrets: [], secretsLoad: SECRETS_LOAD.ready,
  onCreated: vi.fn(), onSecretsChanged: vi.fn(), onSecretsNeeded: vi.fn(),
};
const model: LibraryModel = {
  id: "claude-sonnet-5", provider: "anthropic", context_cap_tokens: 200000,
  input_nanos_per_mtok: 0, cached_input_nanos_per_mtok: 0, output_nanos_per_mtok: 0,
};

afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("provider picker during catalogue loading", () => {
  it.each([CATALOGUE_STATUS.idle, CATALOGUE_STATUS.loading])(
    "holds a disabled picker during %s and enables the same control when ready",
    async (status) => {
      catalogue.status = status;
      catalogue.models = [];
      const view = render(<AddModelEntryDialog {...props} />);
      const user = userEvent.setup();
      await user.click(screen.getByRole("button", { name: "Create model" }));
      const dialog = within(screen.getByRole("dialog"));
      await user.type(dialog.getByLabelText("Name"), "my-provider");
      const provider = dialog.getByRole("combobox", { name: "Provider" });
      expect(provider).toHaveProperty("disabled", true);
      expect(provider.textContent).toContain("Loading providers");
      expect(dialog.queryByRole("textbox", { name: "Provider" })).toBeNull();

      catalogue.models = [model];
      catalogue.status = CATALOGUE_STATUS.ready;
      view.rerender(<AddModelEntryDialog {...props} />);
      expect(dialog.getByRole("combobox", { name: "Provider" })).toBe(provider);
      expect(provider).toHaveProperty("disabled", false);
      expect(dialog.getByLabelText("Name")).toHaveProperty("value", "my-provider");
      await user.click(provider);
      await user.click(screen.getByRole("option", { name: "Anthropic" }));
      expect(provider.textContent).toContain("Anthropic");
    },
  );

  it.each([CATALOGUE_STATUS.error, CATALOGUE_STATUS.ready])(
    "allows free-text entry after a %s catalogue without rows",
    async (status) => {
      catalogue.status = status;
      catalogue.models = [];
      render(<AddModelEntryDialog {...props} />);
      const user = userEvent.setup();
      await user.click(screen.getByRole("button", { name: "Create model" }));
      const provider = within(screen.getByRole("dialog")).getByRole("textbox", { name: "Provider" });
      await user.type(provider, "anthropic");
      expect(provider).toHaveProperty("value", "anthropic");
    },
  );
});
