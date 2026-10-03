import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";
import type { PlatformCatalogEntry } from "@/lib/types";
import type { WorkspaceLibraryEntry } from "@/lib/api/library-types";
import { SOURCE_GITHUB_LINK_LABEL, SOURCE_KIND_GITHUB, SOURCE_KIND_UPLOAD } from "@/components/domain/fleet-library/SourceMark";

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: vi.fn() }) }));
vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({
  patchPlatformLibraryAction: vi.fn(), deletePlatformLibraryAction: vi.fn(),
}));
vi.mock("@/app/(dashboard)/w/[workspaceId]/library/actions", () => ({
  listLibraryEntriesAction: vi.fn(), removeLibraryEntryAction: vi.fn(),
}));

import PlatformCatalogTable from "@/app/(dashboard)/admin/fleet-libraries/components/PlatformCatalogTable";
import WorkspaceLibraryList from "@/app/(dashboard)/w/[workspaceId]/library/components/WorkspaceLibraryList";

const NAME = "reviewer-bundle";
const REPO = "agentsfleet/github-pr-reviewer";
const UPLOAD = "bundle-reviewer";
const HASH = "abc123def456789";
const NOW = Date.UTC(2026, 8, 25);
const HEADERS = ["Name", "Source", "Status", "Time", "Actions"];

function platform(sourceRepo: string, sourceRef = ""): PlatformCatalogEntry {
  return {
    id: NAME, name: NAME, description: "Reviews pull requests.",
    source_repo: sourceRepo, source_ref: sourceRef, visibility: "draft",
    content_hash: HASH, etag: '"v1"', updated_at: NOW,
    requirements: { credentials: [], tools: [], network_hosts: [], trigger_present: false },
    required_credentials_reasons: {},
  };
}

function workspace(kind: string, sourceRef: string): WorkspaceLibraryEntry {
  return {
    id: NAME, name: NAME, description: "Reviews pull requests.",
    source_kind: kind, source_ref: sourceRef, content_hash: HASH, created_at: NOW,
  };
}

function showPlatform(row: PlatformCatalogEntry) {
  render(<TooltipProvider><PlatformCatalogTable entries={[row]} onFetch={vi.fn()} /></TooltipProvider>);
}

function showWorkspace(row: WorkspaceLibraryEntry) {
  render(<TooltipProvider><WorkspaceLibraryList workspaceId="ws_1" entries={[row]} initialCursor={null} /></TooltipProvider>);
}

function headers() {
  return screen.getAllByRole("columnheader").map((header) => header.textContent?.trim());
}

function sourceCell() {
  const row = screen.getAllByRole("row")[1];
  if (!row) throw new Error("Expected a library row");
  const cell = within(row).getAllByRole("cell")[1];
  if (!cell) throw new Error("Expected the Source cell");
  return cell;
}

afterEach(cleanup);

describe("Fleet library table presentation", () => {
  it("keeps the admin columns standard and bundle hashes out of rows", () => {
    showPlatform(platform(REPO, "main"));
    expect(headers()).toEqual(HEADERS);
    expect(screen.queryByText(HASH.slice(0, 12))).toBeNull();
    expect(screen.queryByRole("button", { name: /copy.*bundle hash/i })).toBeNull();
  });

  it("includes a truthful workspace status in the standard column order", () => {
    showWorkspace(workspace(SOURCE_KIND_GITHUB, REPO));
    expect(headers()).toEqual(HEADERS);
    expect(screen.getByText("Ready").getAttribute("title")).toBe("Bundle stored. Available in this workspace gallery.");
  });

  it("draws the same upload icon and inert source text on both surfaces", () => {
    showPlatform(platform(UPLOAD));
    const adminSource = sourceCell().innerHTML;
    expect(within(sourceCell()).queryByRole("link")).toBeNull();
    cleanup();
    showWorkspace(workspace(SOURCE_KIND_UPLOAD, UPLOAD));
    expect(sourceCell().innerHTML).toBe(adminSource);
    expect(within(sourceCell()).getByText(UPLOAD)).toBeTruthy();
    expect(within(sourceCell()).queryByRole("link")).toBeNull();
  });

  it("labels uploads identically even when their stored source is empty", () => {
    showPlatform(platform(""));
    const adminSource = sourceCell().innerHTML;
    expect(within(sourceCell()).getByText(NAME)).toBeTruthy();
    expect(within(sourceCell()).queryByRole("link")).toBeNull();
    cleanup();
    showWorkspace(workspace(SOURCE_KIND_UPLOAD, ""));
    expect(sourceCell().innerHTML).toBe(adminSource);
    expect(within(sourceCell()).getByText(NAME)).toBeTruthy();
    expect(within(sourceCell()).queryByRole("link")).toBeNull();
  });

  it("uses the same GitHub mark while preserving each surface's stored reference", () => {
    showPlatform(platform(REPO, "main"));
    const adminGlyph = sourceCell().querySelector(`[title="${SOURCE_GITHUB_LINK_LABEL}"]`)?.innerHTML;
    expect(adminGlyph).toBeTruthy();
    expect(within(sourceCell()).getByRole("link").getAttribute("href")).toBe(`https://github.com/${REPO}/tree/main`);
    cleanup();
    showWorkspace(workspace(SOURCE_KIND_GITHUB, REPO));
    expect(sourceCell().querySelector(`[title="${SOURCE_GITHUB_LINK_LABEL}"]`)?.innerHTML).toBe(adminGlyph);
    expect(within(sourceCell()).getByRole("link").getAttribute("href")).toBe(`https://github.com/${REPO}`);
  });
});
