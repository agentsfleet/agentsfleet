import React from "react";
import { vi } from "vitest";
import { render } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { PlatformCatalogEntry } from "@/lib/types";
import FleetLibrariesView from "@/app/(dashboard)/admin/fleet-libraries/components/FleetLibrariesView";

export const onboardPlatformLibraryActionMock = vi.fn();
export const patchPlatformLibraryActionMock = vi.fn();
export const deletePlatformLibraryActionMock = vi.fn();

vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({
  onboardPlatformLibraryAction: (...args: unknown[]) => onboardPlatformLibraryActionMock(...args),
  patchPlatformLibraryAction: (...args: unknown[]) => patchPlatformLibraryActionMock(...args),
  deletePlatformLibraryAction: (...args: unknown[]) => deletePlatformLibraryActionMock(...args),
}));
export const captureProductEventMock = vi.fn();
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: (...args: unknown[]) => captureProductEventMock(...args),
}));

export function entry(over: Partial<PlatformCatalogEntry> = {}): PlatformCatalogEntry {
  return {
    id: "platform-ops",
    name: "Platform operations diagnostician",
    description: "Diagnoses platform incidents.",
    source_repo: "agentsfleet/platform-ops",
    source_ref: "main",
    visibility: "draft",
    content_hash: "abc123def456789",
    requirements: {
      credentials: ["fly", "slack"],
      tools: ["http_request"],
      network_hosts: ["api.machines.dev"],
      trigger_present: true,
    },
    required_credentials_reasons: {},
    etag: '"catalog-v1"',
    updated_at: 1_700_000_000_000,
    ...over,
  };
}

export const PUBLISHED = entry({ id: "github-pr-reviewer", name: "Reviewer", visibility: "public" });
export const DRAFT = entry();
export const PUBLISHED_DRAFT = entry({ visibility: "public", etag: '"catalog-v2"' });
export const NO_BUNDLE = entry({ id: "zoho-sprint", name: "Zoho", content_hash: null });
export const MISTYPED_REPO = "agentsfleet/mistyped";

export function renderView(entries: PlatformCatalogEntry[]) {
  render(
    <TooltipProvider>
      <FleetLibrariesView entries={entries} />
    </TooltipProvider>,
  );
}
