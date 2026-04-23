// Reads a two-file zombie bundle (SKILL.md + TRIGGER.md) from a local directory.
// Pure: returns data or throws a typed SkillLoadError. Caller owns user-facing formatting.

import { readFileSync, statSync } from "node:fs";
import { join, basename } from "node:path";

const SKILL_FILENAME = "SKILL.md";
const TRIGGER_FILENAME = "TRIGGER.md";

export class SkillLoadError extends Error {
  constructor(code, detail) {
    super(detail);
    this.code = code;
  }
}

export function loadSkillFromPath(path) {
  if (typeof path !== "string" || path === "") {
    throw new SkillLoadError("ERR_PATH_NOT_FOUND", "<no path provided>");
  }
  let stat;
  try {
    stat = statSync(path);
  } catch (err) {
    if (err.code === "EACCES") throw new SkillLoadError("ERR_PATH_DENIED", path);
    throw new SkillLoadError("ERR_PATH_NOT_FOUND", path);
  }
  if (!stat.isDirectory()) {
    throw new SkillLoadError("ERR_PATH_NOT_FOUND", `${path} (not a directory)`);
  }

  const skillPath = join(path, SKILL_FILENAME);
  const triggerPath = join(path, TRIGGER_FILENAME);

  let skill_md;
  try {
    skill_md = readFileSync(skillPath, "utf-8");
  } catch (err) {
    if (err.code === "EACCES") throw new SkillLoadError("ERR_PATH_DENIED", skillPath);
    throw new SkillLoadError("ERR_SKILL_MISSING", skillPath);
  }

  let trigger_md;
  try {
    trigger_md = readFileSync(triggerPath, "utf-8");
  } catch (err) {
    if (err.code === "EACCES") throw new SkillLoadError("ERR_PATH_DENIED", triggerPath);
    throw new SkillLoadError("ERR_TRIGGER_MISSING", triggerPath);
  }

  const nameMatch = trigger_md.match(/^name:\s*(.+)$/m);
  const name = nameMatch ? nameMatch[1].trim() : basename(path);

  return { skill_md, trigger_md, name };
}
