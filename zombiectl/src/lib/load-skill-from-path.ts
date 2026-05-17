// Reads a two-file zombie bundle (SKILL.md + TRIGGER.md) from a local directory.
// Pure: returns data or throws a typed SkillLoadError. Caller owns user-facing formatting.

import { readFileSync, statSync } from "node:fs";
import { join, basename } from "node:path";

const SKILL_FILENAME = "SKILL.md";
const TRIGGER_FILENAME = "TRIGGER.md";

export type SkillLoadErrorCode =
  | "ERR_PATH_NOT_FOUND"
  | "ERR_PATH_DENIED"
  | "ERR_SKILL_MISSING"
  | "ERR_TRIGGER_MISSING";

export class SkillLoadError extends Error {
  readonly code: SkillLoadErrorCode;

  constructor(code: SkillLoadErrorCode, detail: string) {
    super(detail);
    this.code = code;
    this.name = "SkillLoadError";
  }
}

export interface LoadedSkill {
  skill_md: string;
  trigger_md: string;
  fallback_name: string;
}

function isNodeErrnoException(err: unknown): err is NodeJS.ErrnoException {
  return err instanceof Error && typeof (err as NodeJS.ErrnoException).code === "string";
}

export function loadSkillFromPath(path: string): LoadedSkill {
  if (typeof path !== "string" || path === "") {
    throw new SkillLoadError("ERR_PATH_NOT_FOUND", "<no path provided>");
  }
  let stat;
  try {
    stat = statSync(path);
  } catch (err) {
    if (isNodeErrnoException(err) && err.code === "EACCES") {
      throw new SkillLoadError("ERR_PATH_DENIED", path);
    }
    throw new SkillLoadError("ERR_PATH_NOT_FOUND", path);
  }
  if (!stat.isDirectory()) {
    throw new SkillLoadError("ERR_PATH_NOT_FOUND", `${path} (not a directory)`);
  }

  const skillPath = join(path, SKILL_FILENAME);
  const triggerPath = join(path, TRIGGER_FILENAME);

  let skill_md: string;
  try {
    skill_md = readFileSync(skillPath, "utf-8");
  } catch (err) {
    if (isNodeErrnoException(err) && err.code === "EACCES") {
      throw new SkillLoadError("ERR_PATH_DENIED", skillPath);
    }
    throw new SkillLoadError("ERR_SKILL_MISSING", skillPath);
  }

  let trigger_md: string;
  try {
    trigger_md = readFileSync(triggerPath, "utf-8");
  } catch (err) {
    if (isNodeErrnoException(err) && err.code === "EACCES") {
      throw new SkillLoadError("ERR_PATH_DENIED", triggerPath);
    }
    throw new SkillLoadError("ERR_TRIGGER_MISSING", triggerPath);
  }

  // The canonical zombie name comes back in the install response after the
  // server parses TRIGGER.md frontmatter. The directory basename is only a
  // fallback hint for human-readable CLI output if the server omits it.
  return { skill_md, trigger_md, fallback_name: basename(path) };
}
