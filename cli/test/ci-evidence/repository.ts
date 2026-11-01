import type { EvidenceFixture } from "./fixture.ts";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { REPORT_PATH, gitEnvironment } from "@ci-evidence/history";

/** Real disposable Git history; no network and no repository hook overrides. */
export class EvidenceRepository {
  readonly root = mkdtempSync(join(tmpdir(), "ci-evidence-test-"));
  readonly path = join(this.root, REPORT_PATH);
  readonly manifest = join(this.root, "tests/fixtures/fleets/ci-investigation/cases.json");
  constructor() {
    this.git("init", "--initial-branch=evidence-test");
    this.git("config", "user.name", "Evidence test"); this.git("config", "user.email", "evidence@example.test");
    writeFileSync(join(this.root, "README.md"), "Disposable synthetic evidence repository.\n");
    this.git("add", "README.md"); this.git("commit", "-m", "test: initialize evidence history");
    mkdirSync(dirname(this.path), {recursive: true}); mkdirSync(dirname(this.manifest), {recursive: true});
  }
  git(...args: string[]): string {
    const result = Bun.spawnSync(["git", ...args], {cwd: this.root, env: gitEnvironment(),
      stdin: "ignore", stdout: "pipe", stderr: "pipe", timeout: 10000, maxBuffer: 1024 * 1024});
    if (result.exitCode !== 0 || result.signalCode) throw new Error("Scratch Git operation failed");
    return result.stdout.toString().trim();
  }
  write(fixture: EvidenceFixture): void {
    writeFileSync(this.path, JSON.stringify(fixture.report)); writeFileSync(this.manifest, JSON.stringify(fixture.manifest));
  }
  commit(): string {this.git("add", REPORT_PATH); this.git("commit", "-m", "test: retain synthetic campaign"); return this.git("rev-parse", "HEAD");}
  dispose(): void {rmSync(this.root, {recursive: true, force: true});}
}
