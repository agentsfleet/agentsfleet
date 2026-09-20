import { describe, test, expect } from "bun:test";
import { mkdirSync, mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-00000067e210";
const LIBRARIES = `/v1/workspaces/${WS_ID}/fleet-libraries`;
const LIBRARY_ID = "01900000-0000-7000-8000-0000000aa001";

const SKILL_MD = "---\nname: probe\n---\n# Probe\n";
const TRIGGER_MD = "---\nname: probe\n---\n# Wake rule\n";

const created = (overrides: Record<string, unknown> = {}) => ({
  id: LIBRARY_ID,
  name: "probe",
  visibility: "tenant",
  requirements: { credentials: ["github"], tools: [], network_hosts: [], trigger_present: true },
  ...overrides,
});

const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_library_add" }, fn);

/** A bundle directory on disk; `withTrigger: false` omits TRIGGER.md, which the
 *  daemon treats as optional. */
const withBundle = async <T>(
  withTrigger: boolean,
  fn: (dir: string) => Promise<T>,
): Promise<T> => {
  const dir = mkdtempSync(join(tmpdir(), "af-bundle-"));
  try {
    writeFileSync(join(dir, "SKILL.md"), SKILL_MD);
    if (withTrigger) writeFileSync(join(dir, "TRIGGER.md"), TRIGGER_MD);
    return await fn(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
};

const parseBody = (raw: string | null): Record<string, unknown> =>
  raw === null ? {} : (JSON.parse(raw) as Record<string, unknown>);

describe("library add — source selection", () => {
  test("refuses a bare invocation before any request leaves the process", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["library", "add"], {
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
          ["library", "add", "--github", "owner/repo", "--template", "starter"],
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
          ["library", "add", "--template", "starter", "--ref", "main"],
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
        const code = await runCli(["library", "add", "--github", "not-a-repo"], {
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
          ["library", "add", "--from", join(tmpdir(), "af-does-not-exist-9e3f")],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(5);
        expect(calls).toEqual([]);
      });
    });
  });
});

describe("library add — request shaping", () => {
  test("--github posts a github source carrying the repository", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () => jsonResponse(201, created()),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["library", "add", "--github", "agentsfleet/github-pr-reviewer"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const body = parseBody(calls[0]?.body ?? null);
        expect(body.source_kind).toBe("github");
        expect(body.source_ref).toBe("agentsfleet/github-pr-reviewer");
        expect(body.ref).toBeUndefined();
        const text = out.read();
        expect(text).toContain(LIBRARY_ID);
        // The identifier is printed with the command that consumes it.
        expect(text).toContain("agentsfleet install --library");
      });
    });
  });

  test("--ref rides a github source onto the wire", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () => jsonResponse(201, created()),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["library", "add", "--github", "owner/repo", "--ref", "v1.2.0"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        expect(parseBody(calls[0]?.body ?? null).ref).toBe("v1.2.0");
      });
    });
  });

  test("--from posts an upload carrying both bundle documents", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () => jsonResponse(201, created()),
      };
      await withBundle(true, (dir) =>
        withMockApi(routes, async (apiUrl, calls) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(["library", "add", "--from", dir], {
            stdout: out.stream,
            stderr: err.stream,
            env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
          });
          expect(code).toBe(0);
          const body = parseBody(calls[0]?.body ?? null);
          expect(body.source_kind).toBe("upload");
          // Provenance is the bundle's directory name, not the absolute path:
          // the gallery prints source_ref to every workspace member, and an
          // absolute path carries the operator's home directory with it.
          expect(String(body.source_ref)).not.toContain("/");
          expect(body.skill_markdown).toBe(SKILL_MD);
          expect(body.trigger_markdown).toBe(TRIGGER_MD);
          // The daemon refuses an upload carrying attachments.
          expect(body.support_files).toEqual([]);
        }),
      );
    });
  });

  test("--from omits the trigger key when the bundle has no TRIGGER.md", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () => jsonResponse(201, created()),
      };
      await withBundle(false, (dir) =>
        withMockApi(routes, async (apiUrl, calls) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(["library", "add", "--from", dir], {
            stdout: out.stream,
            stderr: err.stream,
            env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
          });
          expect(code).toBe(0);
          const body = parseBody(calls[0]?.body ?? null);
          expect(body.skill_markdown).toBe(SKILL_MD);
          expect(body).not.toHaveProperty("trigger_markdown");
        }),
      );
    });
  });

  test("--template posts a template source", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () => jsonResponse(201, created()),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["library", "add", "--template", "starter"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const body = parseBody(calls[0]?.body ?? null);
        expect(body.source_kind).toBe("template");
        expect(body.source_ref).toBe("starter");
      });
    });
  });

  test("--replace is not offered, because the workspace plane drops it", async () => {
    await authedScope(async () => {
      // `Destination::Workspace` carries no replace field and the handler
      // states the flag is "deliberately dropped" on this plane. A flag that
      // reaches the daemon and changes nothing is worse than no flag: it reads
      // as a guarantee.
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["library", "add", "--github", "owner/repo", "--replace"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(4);
        expect(err.read()).toContain("Unrecognized flag");
        expect(calls).toEqual([]);
      });
    });
  });

  test("a refused bundle renders the daemon's own sentence, not its log detail", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () =>
          jsonResponse(400, {
            error_code: "UZ-BUNDLE-001",
            detail: "Fleet Bundle is invalid",
            user_message:
              "That Fleet Bundle isn't valid. It's missing `SKILL.md`, or has an unsafe or oversized file. Check the source and try again.",
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["library", "add", "--github", "owner/repo"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(3);
        const text = err.read();
        expect(text).toContain("That Fleet Bundle isn't valid.");
        expect(text).toContain("UZ-BUNDLE-001");
      });
    });
  });
});

describe("library add — machine surface", () => {
  test("--json emits the created entry instead of the prose block", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () => jsonResponse(201, created()),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["library", "add", "--json", "--github", "owner/repo"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const text = out.read();
        const parsed = JSON.parse(text) as { id: string; visibility: string };
        expect(parsed.id).toBe(LIBRARY_ID);
        expect(parsed.visibility).toBe("tenant");
        expect(text).not.toContain("Install it with");
      });
    });
  });
});

describe("library add — degraded daemon answers", () => {
  test("a creation carrying no name falls back to the source reference", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () => jsonResponse(201, { id: LIBRARY_ID }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["library", "add", "--github", "owner/repo"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("owner/repo");
        expect(text).not.toContain("undefined");
      });
    });
  });

  test("a creation carrying no identifier still prints a runnable install line", async () => {
    await authedScope(async () => {
      // Without the placeholder the hint would read "install --library
      // undefined", which is worse than useless: it looks like a command.
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () => jsonResponse(201, { name: "probe" }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["library", "add", "--github", "owner/repo"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("<library_id>");
        expect(text).not.toContain("undefined");
      });
    });
  });
});

describe("library add --from — a bundle an upload cannot carry", () => {
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
        const code = await runCli(["library", "add", "--from", dir], {
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
