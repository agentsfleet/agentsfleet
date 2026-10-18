import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";

vi.mock("@/app/(dashboard)/admin/models/actions", () => ({ setPlatformDefaultAction: vi.fn() }));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: vi.fn() }));

import MakeDefaultDialog from "@/app/(dashboard)/admin/models/components/MakeDefaultDialog";
import { type AdminModel } from "@/lib/api/admin_model_library";

/** A catalogue row whose provider id is a slug with a vendor spelling. */
const MODEL: AdminModel = {
  id: "m1",
  provider: "atlas-cloud",
  model_id: "deepseek-v4p1-flash",
  display_name: "DeepSeek V4.1 Flash",
  advanced: false,
  context_cap_tokens: 1_048_576,
  input_nanos_per_mtok: 300_000_000,
  cached_input_nanos_per_mtok: 30_000_000,
  output_nanos_per_mtok: 1_200_000_000,
};

/** How the dialog names `MODEL.provider`. */
const PROVIDER_SHOWN = "Atlas Cloud";

afterEach(cleanup);

describe("the make-default dialog", () => {
  it("names the provider the way the catalogue row does, not by its slug", () => {
    render(
      React.createElement(
        TooltipProvider,
        null,
        React.createElement(MakeDefaultDialog, { model: MODEL, onOpenChange: vi.fn(), onDone: vi.fn() }),
      ),
    );

    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText(PROVIDER_SHOWN)).toBeTruthy();
    expect(within(dialog).queryByText(MODEL.provider)).toBeNull();
  });

  it("keeps the provider badge, a block element, out of a paragraph", () => {
    render(
      React.createElement(
        TooltipProvider,
        null,
        React.createElement(MakeDefaultDialog, { model: MODEL, onOpenChange: vi.fn(), onDone: vi.fn() }),
      ),
    );

    const badge = within(screen.getByRole("dialog")).getByText(PROVIDER_SHOWN);
    expect(badge.closest("p")).toBeNull();
  });
});
