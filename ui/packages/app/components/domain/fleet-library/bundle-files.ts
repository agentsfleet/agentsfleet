// The upload source posts both markdown bodies inline in the JSON body — the
// daemon has no attachment path for it (the library resolver answers an upload
// carrying files with UploadAttachmentsUnsupported), so "point at a bundle
// folder" has to resolve in the browser. This module is that resolution: the
// files a directory picker handed us in, the two bodies out, or one sentence
// saying why not. It stays framework-free so the refusals can be tested
// without rendering the dialog.

export const SKILL_FILE_NAME = "SKILL.md";
export const TRIGGER_FILE_NAME = "TRIGGER.md";

// Cross-runtime pair: `fleet_runtime/markdown_limits.zig` declares MAX_SOURCE_LEN
// and MAX_TRIGGER_LEN at this same ceiling. Refusing at the daemon's boundary
// rather than a looser one of our own means an oversized body is refused before
// the round-trip instead of after it — and File.text() never pulls it into the
// tab to find out.
const BYTES_PER_KIB = 1024;
const MAX_MARKDOWN_KIB = 200;
export const MAX_SOURCE_LEN = MAX_MARKDOWN_KIB * BYTES_PER_KIB;
export const MAX_TRIGGER_LEN = MAX_MARKDOWN_KIB * BYTES_PER_KIB;

// A directory pick enumerates the whole tree. A bundle directory is tens of
// files; pointed at a home directory or a node_modules tree the FileList copy
// alone locks the tab, long before any per-file guard is consulted.
const MAX_PICKED_FILES = 2_000;

type BundleFileName = typeof SKILL_FILE_NAME | typeof TRIGGER_FILE_NAME;

const NO_BUNDLE_FOUND =
  `That folder holds no ${SKILL_FILE_NAME} or ${TRIGGER_FILE_NAME}. Pick the bundle directory itself.`;

// Each refusal says what the missing file is FOR, because the entry installs
// without it and fails later somewhere unrelated.
const MISSING_CONSEQUENCE: Record<BundleFileName, string> = {
  [SKILL_FILE_NAME]: "the entry takes its name, description and version from that file's frontmatter",
  [TRIGGER_FILE_NAME]:
    "a fleet installed without it declares no tools, no credentials and no approval gate — and an absent gate reads as approve-everything",
};

/** `empty` is a cancelled picker: it carries no message, because nothing went wrong. */
export const BUNDLE_READ = { loaded: "loaded", empty: "empty", refused: "refused" } as const;

/** A `FileList` satisfies this, and so does the array a test hands over. */
type PickedFiles = { readonly length: number } & Iterable<File>;

type Grouped = { folders: Map<string, Map<BundleFileName, File>>; collision: string | null };

export type BundleRead =
  | { status: typeof BUNDLE_READ.loaded; skillMarkdown: string; triggerMarkdown: string }
  | { status: typeof BUNDLE_READ.empty }
  | { status: typeof BUNDLE_READ.refused; reason: string };

/**
 * Resolve the files of one picked directory into a bundle's two bodies.
 *
 * Grouping by directory is what makes "you pointed at the parent of several
 * bundles" a refusal rather than a silent pick of whichever came first.
 */
export async function readBundleFolder(files: PickedFiles | null): Promise<BundleRead> {
  if (files === null || files.length === 0) return { status: BUNDLE_READ.empty };
  if (files.length > MAX_PICKED_FILES) {
    return refuse(`That folder holds ${files.length} files. Pick the bundle directory itself.`);
  }

  const grouped = groupBundleFiles([...files]);
  if (grouped.collision !== null) {
    return refuse(`That folder holds both ${grouped.collision}. Two spellings of one bundle file is ambiguous — remove one.`);
  }
  const folders = [...grouped.folders.entries()];
  if (folders.length > 1) {
    const named = folders.map(([folder]) => folder).sort().join(", ");
    return refuse(`That folder holds more than one bundle (${named}). Pick a single bundle directory.`);
  }
  const only = folders[0];
  if (!only) return refuse(NO_BUNDLE_FOUND);

  const [, entries] = only;
  const skill = entries.get(SKILL_FILE_NAME);
  const trigger = entries.get(TRIGGER_FILE_NAME);
  if (!skill) return refuse(missingReason(SKILL_FILE_NAME));
  if (!trigger) return refuse(missingReason(TRIGGER_FILE_NAME));
  if (skill.size > MAX_SOURCE_LEN) return refuse(oversizeReason(skill));
  if (trigger.size > MAX_TRIGGER_LEN) return refuse(oversizeReason(trigger));

  return {
    status: BUNDLE_READ.loaded,
    skillMarkdown: await skill.text(),
    triggerMarkdown: await trigger.text(),
  };
}

function oversizeReason(file: File): string {
  return `${file.name} is larger than ${MAX_MARKDOWN_KIB} KiB, which is more than a bundle file may carry.`;
}

function refuse(reason: string): BundleRead {
  return { status: BUNDLE_READ.refused, reason };
}

function missingReason(name: BundleFileName): string {
  return `That folder has no ${name} — ${MISSING_CONSEQUENCE[name]}.`;
}

/**
 * Bundle files only, keyed by the directory they sit in, then by canonical name.
 *
 * `collision` names the two raw filenames when one directory holds both spellings
 * of the same bundle file — possible on a case-sensitive filesystem. Keying by the
 * canonical name would otherwise make the last one enumerated win silently, which
 * is the ambiguity this module refuses one level up.
 */
function groupBundleFiles(files: readonly File[]): Grouped {
  const folders = new Map<string, Map<BundleFileName, File>>();
  for (const file of files) {
    const name = canonicalName(file.name);
    if (name === null) continue;
    const folder = folderOf(file);
    if (folder === null) continue;
    const entries = folders.get(folder) ?? new Map<BundleFileName, File>();
    const seen = entries.get(name);
    if (seen) return { folders, collision: `${seen.name} and ${file.name}` };
    entries.set(name, file);
    folders.set(folder, entries);
  }
  return { folders, collision: null };
}

// macOS and Windows filesystems are case-insensitive, so a folder written as
// `skill.md` holds the same file to the person who picked it.
function canonicalName(raw: string): BundleFileName | null {
  const lowered = raw.toLowerCase();
  if (lowered === SKILL_FILE_NAME.toLowerCase()) return SKILL_FILE_NAME;
  if (lowered === TRIGGER_FILE_NAME.toLowerCase()) return TRIGGER_FILE_NAME;
  return null;
}

// A directory pick populates `webkitRelativePath` ("my-bundle/SKILL.md"). An
// empty one means the file did not arrive inside a chosen directory, and this
// surface takes folders only — so it belongs to no bundle and is dropped.
function folderOf(file: File): string | null {
  const cut = file.webkitRelativePath.lastIndexOf("/");
  return cut === -1 ? null : file.webkitRelativePath.slice(0, cut);
}
