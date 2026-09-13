import { describe, expect, it } from "vitest";
import {
  RECONCILE_ATTEMPTS,
  RECONCILE_INTERVAL_MS,
  REPO_ABSENT,
  importLanded,
  reconcileImport,
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

// The stamp only decides the unchanged-bundle case, so every other fixture can
// hold it still and stay about the hash.
const STAMP = 1_700_000_000_000;

const withHash = (hash: string | null, updatedAt: number = STAMP): RepoImportState => ({
  present: true,
  contentHash: hash,
  updatedAt,
});

describe("repoImportState", () => {
  it("should report absent when no row carries the repository", () => {
    expect(repoImportState([entry({ source_repo: OTHER_REPO })], REPO)).toEqual(REPO_ABSENT);
  });

  it("should report absent when the catalog is empty", () => {
    expect(repoImportState([], REPO)).toEqual(REPO_ABSENT);
  });

  it("should report the stored bundle hash when the row exists", () => {
    expect(repoImportState([entry()], REPO)).toEqual({
      present: true,
      contentHash: "1c59e4131FF7",
      updatedAt: 1,
    });
  });

  it("should distinguish a bundle-less row from an absent one", () => {
    // `content_hash IS NULL` is a row an earlier import created and never
    // finished. Collapsing it into absent would let it pass as a success.
    expect(repoImportState([entry({ content_hash: null })], REPO)).toEqual({
      present: true,
      contentHash: null,
      updatedAt: 1,
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

  it("should not conclude landed when neither the bundle nor the stamp moved", () => {
    // The refetch path's whole difficulty: the row was already present with a
    // hash, so presence proves nothing. Nothing was written, so this is the
    // genuinely failed refetch.
    expect(importLanded(withHash("1c59e4131FF7"), withHash("1c59e4131FF7"))).toBe(false);
  });

  it("should conclude landed when a refetch of an unmoved branch bumped the stamp", () => {
    // A branch that has not moved yields the same bundle, so the hash alone
    // would call a completed refetch a failure. The upsert writes updated_at
    // with no equality guard, so a landed import always advances it.
    const before = withHash("1c59e4131FF7", STAMP);
    const after = withHash("1c59e4131FF7", STAMP + RECONCILE_INTERVAL_MS);
    expect(importLanded(before, after)).toBe(true);
  });

  it("should not conclude landed when the stamp went backwards", () => {
    // A stale replica answering an older row is not proof of anything.
    const before = withHash("1c59e4131FF7", STAMP);
    expect(importLanded(before, withHash("1c59e4131FF7", STAMP - 1))).toBe(false);
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

describe("reconcileImport", () => {
  const noSleep = () => Promise.resolve();

  it("should stop at the first read that proves the import landed", async () => {
    const reads: RepoImportState[] = [withHash("1c59e4131FF7", STAMP + 1)];
    let calls = 0;
    const readState = () => {
      calls += 1;
      return Promise.resolve(reads[0] ?? null);
    };
    await expect(reconcileImport(withHash("1c59e4131FF7"), readState, noSleep)).resolves.toBe(true);
    expect(calls).toBe(1);
  });

  it("should keep asking while the catalog still shows the pre-import row", async () => {
    // The read that fires the instant our patience expires lands before the
    // upsert. One read would report a failure the next read contradicts.
    const answers: (RepoImportState | null)[] = [
      withHash("1c59e4131FF7", STAMP),
      withHash("1c59e4131FF7", STAMP),
      withHash("1c59e4131FF7", STAMP + 1),
    ];
    let calls = 0;
    const readState = () => Promise.resolve(answers[calls++] ?? null);
    await expect(reconcileImport(withHash("1c59e4131FF7"), readState, noSleep)).resolves.toBe(true);
    expect(calls).toBe(3);
  });

  it("should carry on past a read that itself failed", async () => {
    const answers: (RepoImportState | null)[] = [null, null, withHash("9b02aa77c0d1")];
    let calls = 0;
    const readState = () => Promise.resolve(answers[calls++] ?? null);
    await expect(reconcileImport(withHash("1c59e4131FF7"), readState, noSleep)).resolves.toBe(true);
  });

  it("should give up after the bounded number of attempts", async () => {
    let calls = 0;
    const readState = () => {
      calls += 1;
      return Promise.resolve(withHash("1c59e4131FF7", STAMP));
    };
    await expect(reconcileImport(withHash("1c59e4131FF7"), readState, noSleep)).resolves.toBe(false);
    expect(calls).toBe(RECONCILE_ATTEMPTS);
  });

  it("should wait between attempts but never before the first", async () => {
    const waits: number[] = [];
    const sleep = (ms: number) => {
      waits.push(ms);
      return Promise.resolve();
    };
    const readState = () => Promise.resolve(withHash("1c59e4131FF7", STAMP));
    await reconcileImport(withHash("1c59e4131FF7"), readState, sleep);
    expect(waits).toEqual(Array(RECONCILE_ATTEMPTS - 1).fill(RECONCILE_INTERVAL_MS));
  });
});
