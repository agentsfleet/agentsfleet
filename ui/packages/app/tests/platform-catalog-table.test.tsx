import React from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import { afterEach, describe, expect, it, vi } from "vitest";

import PlatformCatalogTable from "@/app/(dashboard)/admin/fleet-libraries/components/PlatformCatalogTable";
import type { PlatformCatalogEntry } from "@/lib/types";

vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({
  deletePlatformLibraryAction: vi.fn(),
  patchPlatformLibraryAction: vi.fn(),
}));

function entry(overrides: Partial<PlatformCatalogEntry> = {}): PlatformCatalogEntry {
  return {
    id: "reviewer",
    name: "Pull request reviewer",
    description: "Reviews pull requests",
    source_repo: "agentsfleet/reviewer",
    source_ref: "main",
    visibility: "draft",
    content_hash: "sha256:abc",
    requirements: { credentials: [], tools: [], network_hosts: [], trigger_present: true },
    support_files: [],
    etag: "etag-1",
    updated_at: 1,
    ...overrides,
  };
}

afterEach(() => cleanup());

describe("PlatformCatalogTable sorting", () => {
  it("sorts each catalog data column from its header arrow", () => {
    render(
      <TooltipProvider>
        <PlatformCatalogTable
          entries={[
            entry(),
            entry({ id: "starter", name: "Starter", source_repo: "upload", visibility: "public", content_hash: null }),
          ]}
          onFetch={vi.fn()}
        />
      </TooltipProvider>,
    );

    for (const name of ["Fleet", "Repository", "Status", "Bundle"]) {
      fireEvent.click(screen.getByRole("button", { name }));
      expect(screen.getByRole("columnheader", { name }).getAttribute("aria-sort")).toBe("ascending");
    }
  });
});
