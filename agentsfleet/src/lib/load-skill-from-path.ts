// Reads a two-file zombie bundle (SKILL.md + TRIGGER.md) from a local directory.
// Pure: returns data or throws a typed SkillLoadError. Caller owns user-facing formatting.

import { readFileSync, statSync } from "node:fs";
import { join, basename } from "node:path";

const SKILL_FILENAME = "SKILL.md";
const TRIGGER_FILENAME = "TRIGGER.md";

export type SkillLoadErrorCode =
  | typeof ERR_PATH_NOT_FOUND_2
  | typeof ERR_PATH_DENIED_2
  | typeof ERR_SKILL_MISSING_2
  | typeof ERR_TRIGGER_MISSING_2;

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
  return err instanceof Error && typeof (err as NodeJS.ErrnoException).code === TYPE_STRING;
}

export function loadSkillFromPath(path: string): LoadedSkill {
  if (typeof path !== TYPE_STRING || path === "") {
    throw new SkillLoadError(ERR_PATH_NOT_FOUND_2, "<no path provided>");
  }
  let stat;
  try {
    stat = statSync(path);
  } catch (err) {
    if (isNodeErrnoException(err) && err.code === EACCES_CODE) {
      throw new SkillLoadError(ERR_PATH_DENIED_2, path);
    }
    throw new SkillLoadError(ERR_PATH_NOT_FOUND_2, path);
  }
  if (!stat.isDirectory()) {
    throw new SkillLoadError(ERR_PATH_NOT_FOUND_2, `${path} (not a directory)`);
  }

  const skillPath = join(path, SKILL_FILENAME);
  const triggerPath = join(path, TRIGGER_FILENAME);

  let skill_md: string;
  try {
    skill_md = readFileSync(skillPath, UTF8_ENCODING);
  } catch (err) {
    if (isNodeErrnoException(err) && err.code === EACCES_CODE) {
      throw new SkillLoadError(ERR_PATH_DENIED_2, skillPath);
    }
    throw new SkillLoadError(ERR_SKILL_MISSING_2, skillPath);
  }

  let trigger_md: string;
  try {
    trigger_md = readFileSync(triggerPath, UTF8_ENCODING);
  } catch (err) {
    if (isNodeErrnoException(err) && err.code === EACCES_CODE) {
      throw new SkillLoadError(ERR_PATH_DENIED_2, triggerPath);
    }
    throw new SkillLoadError(ERR_TRIGGER_MISSING_2, triggerPath);
  }

  // The canonical zombie name comes back in the install response after the
  // server parses TRIGGER.md frontmatter. The directory basename is only a
  // fallback hint for human-readable CLI output if the server omits it.
  return { skill_md, trigger_md, fallback_name: basename(path) };
}
const EACCES_CODE = "EACCES" as const;
const ERR_PATH_DENIED_2 = "ERR_PATH_DENIED" as const;
const ERR_PATH_NOT_FOUND_2 = "ERR_PATH_NOT_FOUND" as const;
const ERR_SKILL_MISSING_2 = "ERR_SKILL_MISSING" as const;
const ERR_TRIGGER_MISSING_2 = "ERR_TRIGGER_MISSING" as const;
const TYPE_STRING = "string" as const;
const UTF8_ENCODING = "utf-8" as const;
