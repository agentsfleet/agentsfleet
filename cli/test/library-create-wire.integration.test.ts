// `library create` — what actually goes on the wire once a source is accepted,
// what comes back on the machine surface, and what the command still prints
// when the daemon answers with fields missing.

import { describe, test, expect } from "bun:test";
import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";
import {
  LIBRARIES,
  LIBRARY_ID,
  SKILL_MD,
  TRIGGER_MD,
  created,
  authedScope,
  withBundle,
  parseBody,
} from "./helpers-library-create.ts";

describe("library create — request shaping", () => {
  test("--github posts a github source carrying the repository", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () => jsonResponse(201, created()),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["library", "create", "--github", "agentsfleet/github-pr-reviewer"],
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
          ["library", "create", "--github", "owner/repo", "--ref", "v1.2.0"],
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
          const code = await runCli(["library", "create", "--from", dir], {
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
          const code = await runCli(["library", "create", "--from", dir], {
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
        const code = await runCli(["library", "create", "--template", "starter"], {
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
          ["library", "create", "--github", "owner/repo", "--replace"],
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
        const code = await runCli(["library", "create", "--github", "owner/repo"], {
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

describe("library create — machine surface", () => {
  test("--json emits the created entry instead of the prose block", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () => jsonResponse(201, created()),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["library", "create", "--json", "--github", "owner/repo"],
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

describe("library create — degraded daemon answers", () => {
  test("a creation carrying no name falls back to the source reference", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`POST ${LIBRARIES}`]: () => jsonResponse(201, { id: LIBRARY_ID }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["library", "create", "--github", "owner/repo"], {
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
        const code = await runCli(["library", "create", "--github", "owner/repo"], {
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
