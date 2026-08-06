import { describe, expect, it } from "vitest";

import {
  BUNDLE_READ,
  MAX_SOURCE_LEN,
  MAX_TRIGGER_LEN,
  readBundleFolder,
  SKILL_FILE_NAME,
  TRIGGER_FILE_NAME,
} from "./bundle-files";

const SKILL_BODY = "---\nname: incident-repairer\nversion: 0.1.0\n---\nBody.";
const TRIGGER_BODY = "---\nname: incident-repairer\nx-agentsfleet:\n  triggers:\n---";
const BUNDLE_DIR = "incident-repairer";

// A directory pick is the only shape this surface accepts, and the browser
// signals it through `webkitRelativePath` — which no File constructor sets.
function pickedFile(relativePath: string, body: string, size?: number): File {
  const name = relativePath.slice(relativePath.lastIndexOf("/") + 1);
  const file = new File([body], name, { type: "text/markdown" });
  Object.defineProperty(file, "webkitRelativePath", { value: relativePath });
  if (size !== undefined) Object.defineProperty(file, "size", { value: size });
  return file;
}

/** What a browser hands back for a file chosen on its own: an empty relative path. */
function looseFile(name: string, body: string): File {
  const file = new File([body], name, { type: "text/markdown" });
  Object.defineProperty(file, "webkitRelativePath", { value: "" });
  return file;
}

function wholeBundle(dir = BUNDLE_DIR): File[] {
  return [
    pickedFile(`${dir}/${SKILL_FILE_NAME}`, SKILL_BODY),
    pickedFile(`${dir}/${TRIGGER_FILE_NAME}`, TRIGGER_BODY),
  ];
}

describe("readBundleFolder", () => {
  it("reads both bodies out of one picked directory", async () => {
    const bundle = await readBundleFolder(wholeBundle());
    expect(bundle).toEqual({
      status: BUNDLE_READ.loaded,
      skillMarkdown: SKILL_BODY,
      triggerMarkdown: TRIGGER_BODY,
    });
  });

  it("accepts the case a case-insensitive filesystem hands back", async () => {
    const bundle = await readBundleFolder([
      pickedFile(`${BUNDLE_DIR}/skill.md`, SKILL_BODY),
      pickedFile(`${BUNDLE_DIR}/trigger.md`, TRIGGER_BODY),
    ]);
    expect(bundle.status).toBe(BUNDLE_READ.loaded);
  });

  it("says nothing when the picker was cancelled", async () => {
    expect(await readBundleFolder(null)).toEqual({ status: BUNDLE_READ.empty });
    expect(await readBundleFolder([])).toEqual({ status: BUNDLE_READ.empty });
  });

  it("refuses a folder holding neither file", async () => {
    const bundle = await readBundleFolder([pickedFile(`${BUNDLE_DIR}/README.md`, "hi")]);
    expect(bundle).toMatchObject({ status: BUNDLE_READ.refused });
    expect(bundle).toHaveProperty("reason", expect.stringContaining("Pick the bundle directory itself"));
  });

  it("refuses files that arrived outside any directory", async () => {
    // A browser leaves `webkitRelativePath` empty for a file chosen
    // individually rather than as part of a folder, and this surface takes
    // folders only.
    const loose = [looseFile(SKILL_FILE_NAME, SKILL_BODY), looseFile(TRIGGER_FILE_NAME, TRIGGER_BODY)];
    expect(await readBundleFolder(loose)).toMatchObject({ status: BUNDLE_READ.refused });
  });

  it("refuses the parent of several bundles instead of picking one for you", async () => {
    const bundle = await readBundleFolder([
      ...wholeBundle("crew/incident-investigator"),
      ...wholeBundle("crew/incident-repairer"),
    ]);
    expect(bundle).toHaveProperty(
      "reason",
      "That folder holds more than one bundle (crew/incident-investigator, crew/incident-repairer). Pick a single bundle directory.",
    );
  });

  it("names what a missing TRIGGER.md would have declared", async () => {
    // The load-bearing refusal: an entry without its trigger installs as a
    // fleet with no tools, no credentials and no gate — and no gate is not a
    // smaller install, it is an autonomous one.
    const bundle = await readBundleFolder([pickedFile(`${BUNDLE_DIR}/${SKILL_FILE_NAME}`, SKILL_BODY)]);
    expect(bundle).toHaveProperty("reason", expect.stringContaining("no approval gate"));
    expect(bundle).toHaveProperty("reason", expect.stringContaining(TRIGGER_FILE_NAME));
  });

  it("names what a missing SKILL.md would have named the entry", async () => {
    const bundle = await readBundleFolder([pickedFile(`${BUNDLE_DIR}/${TRIGGER_FILE_NAME}`, TRIGGER_BODY)]);
    expect(bundle).toHaveProperty("reason", expect.stringContaining("frontmatter"));
    expect(bundle).toHaveProperty("reason", expect.stringContaining(SKILL_FILE_NAME));
  });

  it("refuses an oversized SKILL.md rather than reading it into the tab", async () => {
    const bundle = await readBundleFolder([
      pickedFile(`${BUNDLE_DIR}/${SKILL_FILE_NAME}`, SKILL_BODY, MAX_SOURCE_LEN + 1),
      pickedFile(`${BUNDLE_DIR}/${TRIGGER_FILE_NAME}`, TRIGGER_BODY),
    ]);
    expect(bundle).toHaveProperty("reason", expect.stringContaining(SKILL_FILE_NAME));
  });

  it("refuses an oversized TRIGGER.md too, not just the first file checked", async () => {
    // Guarded separately rather than by one loop, so the trigger's own ceiling
    // cannot be lost to a refactor that only ever looks at the skill.
    const bundle = await readBundleFolder([
      pickedFile(`${BUNDLE_DIR}/${SKILL_FILE_NAME}`, SKILL_BODY),
      pickedFile(`${BUNDLE_DIR}/${TRIGGER_FILE_NAME}`, TRIGGER_BODY, MAX_TRIGGER_LEN + 1),
    ]);
    expect(bundle).toHaveProperty("reason", expect.stringContaining(TRIGGER_FILE_NAME));
  });

  it("accepts a file sitting exactly on the ceiling", async () => {
    // The daemon's own limit is inclusive; refusing here would reject a body it
    // would have stored.
    const bundle = await readBundleFolder([
      pickedFile(`${BUNDLE_DIR}/${SKILL_FILE_NAME}`, SKILL_BODY, MAX_SOURCE_LEN),
      pickedFile(`${BUNDLE_DIR}/${TRIGGER_FILE_NAME}`, TRIGGER_BODY, MAX_TRIGGER_LEN),
    ]);
    expect(bundle.status).toBe(BUNDLE_READ.loaded);
  });

  it("refuses a tree far larger than a bundle before copying it", async () => {
    // Pointed at a home directory the picker enumerates everything; the copy
    // alone locks the tab, so the count is checked before the spread.
    const tree = Array.from({ length: 2_001 }, (_, i) => pickedFile(`${BUNDLE_DIR}/f${i}.md`, "x"));
    expect(await readBundleFolder(tree)).toHaveProperty(
      "reason",
      expect.stringContaining("2001 files"),
    );
  });

  it("refuses two spellings of one bundle file instead of picking the later one", async () => {
    // Both can exist side by side on a case-sensitive filesystem, and keying by
    // the canonical name would make whichever enumerated last win silently.
    const bundle = await readBundleFolder([
      pickedFile(`${BUNDLE_DIR}/${SKILL_FILE_NAME}`, SKILL_BODY),
      pickedFile(`${BUNDLE_DIR}/skill.md`, "a stale backup"),
      pickedFile(`${BUNDLE_DIR}/${TRIGGER_FILE_NAME}`, TRIGGER_BODY),
    ]);
    expect(bundle).toHaveProperty("reason", expect.stringContaining("SKILL.md and skill.md"));
  });
});
