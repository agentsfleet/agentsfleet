// The provider badge and the model name both model tables share, rendered on
// their own: a provider reads as its vendor on every table, and a model's id
// is one hover or one copy away whichever table names it.

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";

import { ModelName, PLATFORM_MANAGED_LABEL, PlatformManagedMark, ProviderBadge } from "./ModelCells";

afterEach(() => cleanup());

const KNOWN_PROVIDER = "atlas-cloud";
const UNLISTED_PROVIDER = "together";
const MODEL_ID = "claude-sonnet-5-5";
const TRAILING = "Default";

function renderCell(cell: React.ReactNode) {
  return render(<TooltipProvider>{cell}</TooltipProvider>);
}

describe("ProviderBadge", () => {
  it("spells a listed provider the vendor's way, not as its slug", () => {
    renderCell(<ProviderBadge provider={KNOWN_PROVIDER} />);
    expect(screen.getByText("Atlas Cloud")).toBeTruthy();
    expect(screen.queryByText(KNOWN_PROVIDER)).toBeNull();
  });

  it("title-cases a provider the label table has not met", () => {
    renderCell(<ProviderBadge provider={UNLISTED_PROVIDER} />);
    expect(screen.getByText("Together")).toBeTruthy();
  });

  it("reads Unknown for a row the server sent no provider for", () => {
    renderCell(<ProviderBadge provider={undefined} />);
    expect(screen.getByText("Unknown")).toBeTruthy();
  });

  it("does not shout the name: the badge's uppercase is overridden", () => {
    renderCell(<ProviderBadge provider={KNOWN_PROVIDER} />);
    expect(screen.getByText("Atlas Cloud").className).toContain("normal-case");
  });
});

describe("ModelName", () => {
  it("shows the marketed name, keeps the id on hover and reaches it by a copy button", () => {
    renderCell(<ModelName modelId={MODEL_ID} />);
    expect(screen.getByTitle(MODEL_ID).textContent).toBe("Sonnet 5.5");
    expect(screen.getByRole("button", { name: `Copy model id: ${MODEL_ID}` })).toBeTruthy();
    expect(screen.queryByLabelText(PLATFORM_MANAGED_LABEL)).toBeNull();
  });

  it("carries the platform lock only when the row is a platform admin's", () => {
    renderCell(<ModelName modelId={MODEL_ID} managedByPlatform />);
    expect(screen.getByLabelText(PLATFORM_MANAGED_LABEL)).toBeTruthy();
    expect(screen.getByTitle(MODEL_ID)).toBeTruthy();
  });

  it("renders what the table puts after the name", () => {
    renderCell(<ModelName modelId={MODEL_ID} trailing={<span>{TRAILING}</span>} />);
    expect(screen.getByText(TRAILING)).toBeTruthy();
  });
});

describe("PlatformManagedMark", () => {
  it("is the lock with its meaning as its accessible name", () => {
    renderCell(<PlatformManagedMark />);
    expect(screen.getByLabelText(PLATFORM_MANAGED_LABEL)).toBeTruthy();
  });
});
