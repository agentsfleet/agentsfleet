// `library create` — the invocations it refuses, and the bundles it will not
// upload. Every case asserts the refusal happened before a request left the
// process: a source that is wrong is cheaper to reject locally than to send.

import { describe, test, expect } from "bun:test";
import { mkdirSync, mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse } from "./helpers-mock-api.ts";
import { LIBRARIES, SKILL_MD, TRIGGER_MD, created, authedScope } from "./helpers-library-create.ts";

describe("library create — source selection", () => {
  test("refuses a bare invocation before any request leaves the process", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["library", "create"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(4);
        expect(err.read()).toContain("exactly one of --github, --from, or --template");
        expect(calls).toEqual([]);
      });
    });
  });

  test("refuses two sources before any request leaves the process", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["library", "create", "--github", "owner/repo", "--template", "starter"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(4);
        expect(calls).toEqual([]);
      });
    });
  });

  test("refuses --ref on a source that has no repository", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["library", "create", "--template", "starter", "--ref", "main"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(4);
        expect(err.read()).toContain("--github only");
        expect(calls).toEqual([]);
      });
    });
  });

  test("refuses a --github value that is not owner/repo", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["library", "create", "--github", "not-a-repo"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(4);
        expect(err.read()).toContain("owner/repo");
        expect(calls).toEqual([]);
      });
    });
  });

  test("refuses a --from path that is not a bundle, without a request", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["library", "create", "--from", join(tmpdir(), "af-does-not-exist-9e3f")],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(5);
        expect(calls).toEqual([]);
      });
    });
  });
});

describe("library create --from — a bundle an upload cannot carry", () => {
  const withExtras = async <T>(
    extra: { readonly dir?: string; readonly file?: string },
    fn: (dir: string) => Promise<T>,
  ): Promise<T> => {
    const dir = mkdtempSync(join(tmpdir(), "af-bundle-extra-"));
    try {
      writeFileSync(join(dir, "SKILL.md"), SKILL_MD);
      writeFileSync(join(dir, "TRIGGER.md"), TRIGGER_MD);
      if (extra.dir) {
        mkdirSync(join(dir, extra.dir));
        writeFileSync(join(dir, extra.dir, "owasp.md"), "# checklist\n");
      }
      if (extra.file) writeFileSync(join(dir, extra.file), "helper\n");
      return await fn(dir);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  };

  const addFrom = async (dir: string) => {
    let captured = { code: 0, err: "", calls: 0 };
    await withMockApi(
      { [`POST ${LIBRARIES}`]: () => jsonResponse(201, created()) },
      async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["library", "create", "--from", dir], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        captured = { code, err: err.read(), calls: calls.length };
      },
    );
    return captured;
  };

  test("refuses a bundle with a support directory instead of dropping it", async () => {
    await authedScope(async () => {
      // `tests/fixtures/fleetbundle/security-reviewer` ships `checklists/`, so
      // this is the ordinary shape. Reading the two root documents and
      // reporting success installs a Fleet whose instructions reference files
      // that were never uploaded.
      const r = await withExtras({ dir: "checklists" }, addFrom);
      expect(r.code).toBe(4);
      expect(r.err).toContain("checklists/");
      expect(r.err).toContain("--github");
      expect(r.calls).toBe(0);
    });
  });

  test("refuses a bundle with a stray support file", async () => {
    await authedScope(async () => {
      const r = await withExtras({ file: "helper.py" }, addFrom);
      expect(r.code).toBe(4);
      expect(r.err).toContain("helper.py");
      expect(r.calls).toBe(0);
    });
  });

  test("a dotfile is not a support file and does not block the upload", async () => {
    await authedScope(async () => {
      // `.gitignore` sits in real bundle repositories and is not content the
      // Fleet reads; the daemon's own archive reader skips dot-prefixed paths.
      const r = await withExtras({ file: ".gitignore" }, addFrom);
      expect(r.code).toBe(0);
      expect(r.calls).toBe(1);
    });
  });
});
