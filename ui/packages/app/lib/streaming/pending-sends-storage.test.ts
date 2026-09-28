import { afterEach, describe, expect, it } from "vitest";
import { purgeOtherUsers } from "./pending-sends-storage";

const PREFIX = "agentsfleet:pending-sends";
const SUBJECT = "user_ledger";
// Starts with `SUBJECT`: only the separator after a subject tells the two apart.
const LONGER_SUBJECT = `${SUBJECT}_2`;
const OWN_KEYS = [`${PREFIX}:${SUBJECT}:ws_a:fleet_a`, `${PREFIX}:${SUBJECT}:ws_b:fleet_b`];
const OTHER_USERS_KEYS = [`${PREFIX}:user_gone:ws_a:fleet_a`, `${PREFIX}:${LONGER_SUBJECT}:ws_a:fleet_a`];
const UNRELATED_KEY = "someone-else:setting";

afterEach(() => {
  window.localStorage.clear();
});

describe("purgeOtherUsers", () => {
  it("removes every other user's ledger and keeps this user's and every unrelated key", () => {
    for (const key of [...OWN_KEYS, ...OTHER_USERS_KEYS, UNRELATED_KEY]) window.localStorage.setItem(key, "[]");
    purgeOtherUsers(SUBJECT);
    expect(Object.keys(window.localStorage).sort()).toEqual([...OWN_KEYS, UNRELATED_KEY].sort());
  });
});
