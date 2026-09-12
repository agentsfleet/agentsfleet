import { describe, expect, it } from "vitest";
import {
  REPO_ABSENT,
  importLanded,
  repoImportState,
  type RepoImportState,
} from "@/app/(dashboard)/admin/fleet-libraries/import-reconcile";
import type { PlatformCatalogEntry } from "@/lib/types";

const REPO = "agentsfleet/github-pr-reviewer";
const OTHER_REPO = "agentsfleet/incident-responder";

function entry(overrides: Partial<PlatformCatalogEntry> = {}): PlatformCatalogEntry {
  return {
    id: "github-pr-reviewer",
    name: "GitHub PR reviewer",
    description: "Reviews pull requests.",
    source_repo: REPO,
    source_ref: "main",
    visibility: "public",
    content_hash: "1c59e4131FF7",
    requirements: { credentials: [] } as unknown as PlatformCatalogEntry["requirements"],
    etag: "W/\"1\"",
    updated_at: 1,
    ...overrides,
  };
}

const withHash = (hash: string | null): RepoImportState => ({ present: true, contentHash: hash });

describe("repoImportState", () => {
  it("should report absent when no row carries the repository", () => {
    expect(repoImportState([entry({ source_repo: OTHER_REPO })], REPO)).toEqual(REPO_ABSENT);
  });

  it("should report absent when the catalog is empty", () => {
    expect(repoImportState([], REPO)).toEqual(REPO_ABSENT);
  });

  it("should report the stored bundle hash when the row exists", () => {
    expect(repoImportState([entry()], REPO)).toEqual({ present: true, contentHash: "1c59e4131FF7" });
  });

  it("should distinguish a bundle-less row from an absent one", () => {
    // `content_hash IS NULL` is a row an earlier import created and never
    // finished. Collapsing it into absent would let it pass as a success.
    expect(repoImportState([entry({ content_hash: null })], REPO)).toEqual({
      present: true,
      contentHash: null,
    });
  });

  it("should match on the repository rather than on position", () => {
    const entries = [entry({ source_repo: OTHER_REPO, content_hash: "aaaa" }), entry()];
    expect(repoImportState(entries, REPO).contentHash).toBe("1c59e4131FF7");
  });
});

describe("importLanded", () => {
  it("should conclude landed when a fresh row appeared carrying a bundle", () => {
    expect(importLanded(REPO_ABSENT, withHash("1c59e4131FF7"))).toBe(true);
  });

  it("should not conclude landed when the catalog still has no such row", () => {
    expect(importLanded(REPO_ABSENT, REPO_ABSENT)).toBe(false);
  });

  it("should not conclude landed when the new row carries no bundle", () => {
    // The row exists but the import never reached object storage. Reporting
    // success here would point the operator at a row they cannot publish.
    expect(importLanded(REPO_ABSENT, withHash(null))).toBe(false);
  });

  it("should not conclude landed when a refetch left the bundle unchanged", () => {
    // The refetch path's whole difficulty: the row was already present with a
    // hash, so presence proves nothing. This is the case that would otherwise
    // report a genuinely failed refetch as a success.
    expect(importLanded(withHash("1c59e4131FF7"), withHash("1c59e4131FF7"))).toBe(false);
  });

  it("should conclude landed when a refetch replaced the bundle", () => {
    expect(importLanded(withHash("1c59e4131FF7"), withHash("9b02aa77c0d1"))).toBe(true);
  });

  it("should conclude landed when an existing bundle-less row was filled", () => {
    expect(importLanded(withHash(null), withHash("1c59e4131FF7"))).toBe(true);
  });

  it("should not conclude landed when a bundle-less row stayed bundle-less", () => {
    expect(importLanded(withHash(null), withHash(null))).toBe(false);
  });

  it("should not conclude landed when the row disappeared while we waited", () => {
    expect(importLanded(withHash("1c59e4131FF7"), REPO_ABSENT)).toBe(false);
  });
});
