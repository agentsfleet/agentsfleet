import { expect, test } from "bun:test";
import { eventTime } from "../src/lib/event-time.ts";

const NOW = Date.parse("2026-11-10T02:00:00Z");
test.each([["15s", "2026-11-10T01:59:45.000Z"], ["30m", "2026-11-10T01:30:00.000Z"],
  ["2h", "2026-11-10T00:00:00.000Z"], ["7d", "2026-11-03T02:00:00.000Z"], ["0s", "2026-11-10T02:00:00.000Z"]])(
  "resolves %s against one clock reading", (input, expected) => expect(eventTime(input, NOW, true)).toBe(expected),
);
test.each(["2026-11-10T02:00:00Z", "2026-11-10T07:30:00+05:30", "2024-02-29T00:00:00.123456789Z"])(
  "preserves the precision and offset of %s", input => expect(eventTime(input, NOW, false)).toBe(input),
);
test.each(["", "2h", "2026-02-29T00:00:00Z", "2026-13-01T00:00:00Z", "2026-11-10T25:00:00Z", "2026-11-10", "🙂"])(
  "refuses invalid absolute bound %s", input => expect(eventTime(input, NOW, false)).toBeUndefined(),
);
test.each(["-1s", "1.5h", "2h30m", "99999999999999999999d", "99999999d"])(
  "refuses unsupported relative bound %s", input => expect(eventTime(input, NOW, true)).toBeUndefined(),
);
