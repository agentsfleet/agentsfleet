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
  context_cap_tokens: 1_048_576,
  input_nanos_per_mtok: 300_000_000,
  cached_input_nanos_per_mtok: 30_000_000,
  output_nanos_per_mtok: 1_200_000_000,
};

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
    expect(within(dialog).getByText("Atlas Cloud")).toBeTruthy();
    expect(within(dialog).queryByText(MODEL.provider)).toBeNull();
  });
});
