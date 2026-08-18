import { describe, expect, it } from "vitest";

import {
  EMPTY_LIBRARY_SOURCE,
  librarySourcePayload,
  librarySourceSchema,
  SKILL_REQUIRED,
  TRIGGER_REQUIRED,
  type LibrarySourceValues,
} from "./library-source-form";

const REPO = "agentsfleet/github-pr-reviewer";
const SKILL_BODY = "---\nname: incident-responder\n---\nBody.";
const TRIGGER_BODY = "---\nname: incident-responder\nx-agentsfleet:\n---";
const STORED_REF = "release-2";

function githubValues(overrides: Partial<LibrarySourceValues> = {}): LibrarySourceValues {
  return { ...EMPTY_LIBRARY_SOURCE, source_ref: REPO, ...overrides };
}

function uploadValues(overrides: Partial<LibrarySourceValues> = {}): LibrarySourceValues {
  return {
    ...EMPTY_LIBRARY_SOURCE,
    source_kind: "upload",
    skill_markdown: SKILL_BODY,
    trigger_markdown: TRIGGER_BODY,
    ...overrides,
  };
}

function issuePaths(values: LibrarySourceValues): string[] {
  const parsed = librarySourceSchema.safeParse(values);
  return parsed.success ? [] : parsed.error.issues.map((issue) => issue.path.join("."));
}

describe("librarySourceSchema", () => {
  it("accepts an owner/repo source on the GitHub side", () => {
    expect(librarySourceSchema.safeParse(githubValues()).success).toBe(true);
  });

  it("rejects a source that is not owner/repo", () => {
    expect(issuePaths(githubValues({ source_ref: "notarepo" }))).toEqual(["source_ref"]);
  });

  // The two sides are validated by `source_kind`, not by which fields carry text.
  // A repository left half-typed on the GitHub tab must not block an upload.
  it("ignores an unusable repository once the source is an upload", () => {
    expect(issuePaths(uploadValues({ source_ref: "notarepo" }))).toEqual([]);
  });

  it("ignores empty bundle bodies while the source is GitHub", () => {
    expect(issuePaths(githubValues({ skill_markdown: "", trigger_markdown: "" }))).toEqual([]);
  });

  // A bundle uploaded without its TRIGGER.md installs as a fleet declaring no
  // tools, no credentials and no gate — and an absent gate reads as
  // approve-everything. Both bodies are required together, and the operator is
  // told about both at once rather than one per submit.
  it("demands both bundle bodies on an upload, naming each", () => {
    const parsed = librarySourceSchema.safeParse(
      uploadValues({ skill_markdown: "", trigger_markdown: "   " }),
    );
    expect(parsed.success).toBe(false);
    const messages = parsed.success ? [] : parsed.error.issues.map((issue) => issue.message);
    expect(messages).toEqual([SKILL_REQUIRED, TRIGGER_REQUIRED]);
  });
});

describe("librarySourcePayload", () => {
  it("sends the repository for a GitHub source", () => {
    expect(librarySourcePayload(githubValues())).toEqual({
      source_kind: "github",
      source_ref: REPO,
    });
  });

  it("pins the stored ref when the caller is refetching a row", () => {
    expect(librarySourcePayload(githubValues(), { ref: STORED_REF })).toEqual({
      source_kind: "github",
      source_ref: REPO,
      ref: STORED_REF,
    });
  });

  it("sends both bundle bodies and no repository for an upload", () => {
    expect(librarySourcePayload(uploadValues())).toEqual({
      source_kind: "upload",
      skill_markdown: SKILL_BODY,
      trigger_markdown: TRIGGER_BODY,
    });
  });

  // `resolveUpload` refuses a request carrying a ref with InvalidSourceRef:
  // pasted bytes came from no revision. The builder must drop a ref the caller
  // passes rather than hand the daemon a request it will reject.
  it("never lets a ref ride an upload, even when the caller supplies one", () => {
    expect(librarySourcePayload(uploadValues(), { ref: STORED_REF })).not.toHaveProperty("ref");
  });

  it("omits replace unless the operator confirmed the overwrite", () => {
    expect(librarySourcePayload(githubValues(), { replace: false })).not.toHaveProperty("replace");
    expect(librarySourcePayload(uploadValues(), { replace: false })).not.toHaveProperty("replace");
  });

  // An uploaded bundle stores no repository, so a name a GitHub-sourced row
  // already owns still collides — the confirm-and-retry has to work from either
  // tab or the Replace button retries into the same refusal forever.
  it("carries replace on both sources once it is confirmed", () => {
    expect(librarySourcePayload(githubValues(), { replace: true })).toMatchObject({ replace: true });
    expect(librarySourcePayload(uploadValues(), { replace: true })).toMatchObject({ replace: true });
  });
});
