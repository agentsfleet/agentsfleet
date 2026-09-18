import { describe, test, expect } from "bun:test";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
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
        expect(body.replace).toBe(false);
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

  test("--replace sets the overwrite flag the daemon reads", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () => jsonResponse(201, created()),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["library", "add", "--github", "owner/repo", "--replace"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        expect(parseBody(calls[0]?.body ?? null).replace).toBe(true);
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
