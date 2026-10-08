import * as fs from "node:fs";
import * as path from "node:path";

// The release-gate suites read the workflow sources straight off disk; this
// file sits at ui/packages/app/tests/helpers/, five levels below the root.
export const REPO_ROOT = path.join(__dirname, "../../../../..");
export const WORKFLOWS_DIR = path.join(REPO_ROOT, ".github/workflows");
const DEPLOY_DEV_WORKFLOW = path.join(WORKFLOWS_DIR, "deploy-dev.yml");

/** The dev pipeline's caller workflow, which owns the release verdict. */
export function deployDevYaml(): string {
  return fs.readFileSync(DEPLOY_DEV_WORKFLOW, "utf8");
}

/** Every file of the dev pipeline (caller + called stages), concatenated. */
export function deployDevFamily(): string {
  return fs
    .readdirSync(WORKFLOWS_DIR)
    .filter((f) => f.startsWith("deploy-dev") && f.endsWith(".yml"))
    .sort()
    .map((f) => fs.readFileSync(path.join(WORKFLOWS_DIR, f), "utf8"))
    .join("\n");
}
