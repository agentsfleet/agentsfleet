import { describe, expect, it } from "vitest";
import {
  CATALOG_DRAFT,
  CATALOG_PUBLIC,
  CATALOG_STATUS_BROKEN,
  CATALOG_STATUS_DRAFT,
  CATALOG_STATUS_NO_BUNDLE,
  CATALOG_STATUS_PUBLISHED,
  catalogStatus,
  type CatalogVisibility,
  type PlatformCatalogEntry,
} from "@/lib/types";
import { rowActions, statusView } from "./catalog-status";
import { STATUS_HELP_PUBLISHED, STATUS_LABEL_PUBLISHED } from "../library-copy";

// The visibility literals and the status labels are spelled ONCE, in the modules
// that own them. A test that re-spells "public" or "Published" is a second source
// for the fact under test, and would keep passing after a rename that broke the
// column (RULE UFS).
const TONE_GREEN = "green";
const TONE_AMBER = "amber";
const TONE_DESTRUCTIVE = "destructive";

// A row is a point on two axes — is it public, and does it hold a bundle. The
// status must be total over both, because the gallery and install queries key off
// BOTH (`visibility = 'public' AND content_hash IS NOT NULL`). A status that reads
// only visibility can promise a fleet the install path will refuse.
function entry(visibility: CatalogVisibility, content_hash: string | null): PlatformCatalogEntry {
  return {
    id: "github-pr-reviewer",
    name: "GitHub Pull Request reviewer",
    description: "Reviews pull requests.",
    source_repo: "agentsfleet/github-pr-reviewer",
    source_ref: "main",
    visibility,
    content_hash,
    requirements: { credentials: [], tools: [], network_hosts: [], trigger_present: true },
    support_files: [],
    etag: '"catalog-v1"',
    updated_at: 1_716_000_000_000,
  };
}

const HASH = "b7f2c1d9e4a8";

describe("catalogStatus", () => {
  // Dimension 1.1 — the hole M130 closes. A hand-inserted row (the API refuses to
  // create one: publishing checks the hash in SQL) is public with no bundle, and
  // the old derivation called it "published" on visibility alone.
  it("calls a public row with no bundle broken, not published", () => {
    expect(catalogStatus(entry(CATALOG_PUBLIC, null))).toBe(CATALOG_STATUS_BROKEN);
  });

  it("is total over every visibility x bundle combination", () => {
    expect(catalogStatus(entry(CATALOG_PUBLIC, HASH))).toBe(CATALOG_STATUS_PUBLISHED);
    expect(catalogStatus(entry(CATALOG_PUBLIC, null))).toBe(CATALOG_STATUS_BROKEN);
    expect(catalogStatus(entry(CATALOG_DRAFT, HASH))).toBe(CATALOG_STATUS_DRAFT);
    expect(catalogStatus(entry(CATALOG_DRAFT, null))).toBe(CATALOG_STATUS_NO_BUNDLE);
  });

  // Server parity: every server guard is `content_hash IS NOT NULL`, so an
  // empty-string hash IS a bundle here too. Treating "" as bundle-less would
  // hide a Publish the API accepts — the exact lie-class this module ends.
  it("counts an empty-string hash as a bundle, exactly as the server does", () => {
    expect(catalogStatus(entry(CATALOG_PUBLIC, ""))).toBe(CATALOG_STATUS_PUBLISHED);
    expect(catalogStatus(entry(CATALOG_DRAFT, ""))).toBe(CATALOG_STATUS_DRAFT);
  });
});

describe("statusView", () => {
  // Dimension 1.2 — the badge is where the lie was actually told. "Live in every
  // workspace gallery" on a row the gallery query filters out.
  it("never claims a bundle-less row is live in every workspace gallery", () => {
    const view = statusView(entry(CATALOG_PUBLIC, null));
    expect(view.help).not.toBe(STATUS_HELP_PUBLISHED);
    expect(view.help).not.toContain("Live in every workspace");
    expect(view.label).not.toBe(STATUS_LABEL_PUBLISHED);
  });

  it("presents the fault as destructive, not as the draft amber", () => {
    expect(statusView(entry(CATALOG_PUBLIC, null)).tone).toBe(TONE_DESTRUCTIVE);
    expect(statusView(entry(CATALOG_DRAFT, HASH)).tone).toBe(TONE_AMBER);
    expect(statusView(entry(CATALOG_PUBLIC, HASH)).tone).toBe(TONE_GREEN);
  });

  it("tells the operator how to resolve the fault", () => {
    const help = statusView(entry(CATALOG_PUBLIC, null)).help;
    expect(help).toContain("Fetch a bundle");
    expect(help).toContain("unpublish");
  });
});

describe("rowActions", () => {
  // Dimension 1.3 — the broken row must be recoverable, and must not offer a
  // button the route will refuse.
  it("offers a broken row the two actions that resolve it", () => {
    const actions = rowActions(entry(CATALOG_PUBLIC, null));
    expect(actions.canUnpublish).toBe(true); // makes it honest
    expect(actions.canPublish).toBe(false); // nothing to serve (UZ-CATALOG-002)
  });

  // The trap: a broken row IS public, so DELETE_CATALOG_DRAFT (WHERE visibility <>
  // 'public') refuses it exactly as it refuses a published one. Keying Delete on
  // `status !== published` would render a button the server answers with a 409.
  it("withholds Delete from a broken row — the route refuses it (UZ-CATALOG-003)", () => {
    expect(rowActions(entry(CATALOG_PUBLIC, null)).canDelete).toBe(false);
    expect(rowActions(entry(CATALOG_PUBLIC, HASH)).canDelete).toBe(false);
  });

  it("keeps the published and draft verdicts unchanged", () => {
    expect(rowActions(entry(CATALOG_PUBLIC, HASH))).toEqual({
      canPublish: false,
      canUnpublish: true,
      canDelete: false,
    });
    expect(rowActions(entry(CATALOG_DRAFT, HASH))).toEqual({
      canPublish: true,
      canUnpublish: false,
      canDelete: true,
    });
    // No bundle: nothing to publish, but safe to delete — it is not public.
    expect(rowActions(entry(CATALOG_DRAFT, null))).toEqual({
      canPublish: false,
      canUnpublish: false,
      canDelete: true,
    });
  });
});
