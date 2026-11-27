import { afterEach, beforeEach, expect, test } from "bun:test";
import { PlatformFixture, WORKSPACE_ID } from "./helpers-platform.ts";

const ROTATE_PATH = `/v1/admin/workspaces/${WORKSPACE_ID}/checkpoint-key/rotate`;

let fixture: PlatformFixture;
beforeEach(() => {fixture = new PlatformFixture();});
afterEach(() => fixture.dispose());

test("rotates a workspace's checkpoint key and prints what the rotation retired", async () => {
  fixture.respond = () => Response.json({retired_generations: 3, retired_objects: 12});
  const result = await fixture.run(["checkpoint-key", "rotate", WORKSPACE_ID, "--json"]);
  expect(result.code).toBe(0);
  expect(JSON.parse(result.stdout)).toEqual({workspace_id: WORKSPACE_ID, retired_generations: 3, retired_objects: 12});
  expect(fixture.calls.map(call => `${call.method} ${call.url.pathname}`)).toEqual([`POST ${ROTATE_PATH}`]);
  expect(fixture.calls[0]?.body).toBeUndefined();
});

test("names the retired checkpoints and stored objects on a terminal", async () => {
  fixture.respond = () => Response.json({retired_generations: 1, retired_objects: 0});
  const result = await fixture.run(["checkpoint-key", "rotate", WORKSPACE_ID], "", true);
  expect(result.code).toBe(0);
  expect(result.stdout).toContain("Checkpoint key rotated");
  expect(result.stdout).toMatch(/Checkpoints retired\s+·\s+1\n/);
  expect(result.stdout).toMatch(/Stored objects to delete\s+·\s+0\n/);
});

test("surfaces the refusal a caller without the platform scope receives", async () => {
  fixture.respond = () => Response.json({error_code: "UZ-AUTH-022", user_message: "Missing platform key administration scope"}, {status: 403});
  const result = await fixture.run(["checkpoint-key", "rotate", WORKSPACE_ID, "--json"]);
  expect(result.code).toBe(3);
  expect(JSON.parse(result.stderr).error.code).toBe("UZ-AUTH-022");
  expect(result.stdout).toBe(""); expect(fixture.calls).toHaveLength(1);
});

test("refuses a workspace that is not an identifier, or a reply of another shape, without a second call", async () => {
  const malformed = await fixture.run(["checkpoint-key", "rotate", "not-an-id", "--json"]);
  expect(malformed.code).toBe(4); expect(fixture.calls).toEqual([]);
  fixture.respond = () => Response.json({retired_generations: "three"});
  const unreadable = await fixture.run(["checkpoint-key", "rotate", WORKSPACE_ID, "--json"]);
  expect(unreadable.code).not.toBe(0); expect(unreadable.stdout).toBe(""); expect(fixture.calls).toHaveLength(1);
});
