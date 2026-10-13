// The provider badge and the model name both model tables share, rendered on
// their own: a provider reads as its vendor on every table, and a model's id
// is one hover or one copy away whichever table names it.

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";

import { ModelName, PLATFORM_MANAGED_LABEL, PlatformManagedMark, ProviderBadge } from "./ModelCells";

afterEach(() => cleanup());

const KNOWN_PROVIDER = "atlas-cloud";
const UNLISTED_PROVIDER = "lambda-labs";
const MODEL_ID = "claude-sonnet-5-5";
const TRAILING = "Default";
/** The neutral badge fill, and the info colour a status badge carries. */
const NEUTRAL = "bg-muted";
const STATUS_INFO = "text-info";
/** The fixed name width a table opts into to line up what follows the name. */
const TRIMMED = "sm:max-w-trim";
/** How a value the viewer cannot change, or that is absent, reads. */
const MUTED = "text-muted-foreground";

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
    expect(screen.getByText("Lambda Labs")).toBeTruthy();
  });

  it("reads a plain, muted Unknown for a row the server sent no provider for", () => {
    renderCell(<ProviderBadge provider={undefined} />);
    const unknown = screen.getByText("Unknown");
    expect(unknown.className).toContain(MUTED);
    expect(unknown.className).not.toContain("normal-case");
  });

  it("reads as a name, not a status: the neutral badge, not a status colour", () => {
    renderCell(<ProviderBadge provider={KNOWN_PROVIDER} />);
    const classes = screen.getByText("Atlas Cloud").className;
    expect(classes).toContain(NEUTRAL);
    expect(classes).not.toContain(STATUS_INFO);
  });

  it("does not shout the name: the badge's uppercase is overridden", () => {
    renderCell(<ProviderBadge provider={KNOWN_PROVIDER} />);
    expect(screen.getByText("Atlas Cloud").className).toContain("normal-case");
  });
});

describe("ModelName", () => {
  it("sizes the name to its column unless the table asks for the trimmed width", () => {
    const { unmount } = renderCell(<ModelName modelId={MODEL_ID} />);
    expect(screen.getByTitle(MODEL_ID).className).not.toContain(TRIMMED);
    unmount();

    renderCell(<ModelName modelId={MODEL_ID} trimmed />);
    expect(screen.getByTitle(MODEL_ID).className).toContain(TRIMMED);
  });

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

  it("mutes the name of a row a tenant admin cannot edit, and only that row", () => {
    renderCell(<ModelName modelId={MODEL_ID} managedByPlatform />);
    expect(screen.getByTitle(MODEL_ID).className).toContain(MUTED);

    cleanup();
    renderCell(<ModelName modelId={MODEL_ID} />);
    expect(screen.getByTitle(MODEL_ID).className).not.toContain(MUTED);
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
