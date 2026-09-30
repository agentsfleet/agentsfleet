import { describe, expect, it } from "vitest";
import type { WorkspaceMember } from "@/lib/api/tenant-members";
import { ACTOR, SENDER, senderLabelFor } from "./event-summary";
import { OWN_SENDER, isNamedTeammate, nameSender, namedMembers, senderNamesFrom } from "./sender-names";

const FLEET = "MARY-001";
const VIEWER = "user_john";
const BOB = `${ACTOR.STEER_PREFIX}user_bob`;
const NAMELESS = `${ACTOR.STEER_PREFIX}user_nameless`;
const STRANGER = `${ACTOR.STEER_PREFIX}user_2abcdefghijklmnop`;

function member(actor: string, display_name: string | null): WorkspaceMember {
  return { user_id: actor, display_name, role: "member", actor };
}

const NAMES = senderNamesFrom(
  VIEWER,
  namedMembers([member(BOB, "Bob"), member(NAMELESS, null), member(`${ACTOR.STEER_PREFIX}user_blank`, "  ")]),
);

describe("nameSender", () => {
  it("test_sender_labels_name_members", () => {
    expect(nameSender(`${ACTOR.STEER_PREFIX}${VIEWER}`, FLEET, NAMES)).toBe(OWN_SENDER);
    expect(nameSender(BOB, FLEET, NAMES)).toBe("Bob");
    expect(nameSender(ACTOR.API_STEER, FLEET, NAMES)).toBe(SENDER.API);
    // A person the thread cannot name keeps the fallback, never an identifier.
    expect(nameSender(STRANGER, FLEET, NAMES)).toBe(senderLabelFor(STRANGER, FLEET));
    expect(nameSender(STRANGER, FLEET, NAMES)).not.toContain("user_");
  });

  it("names the viewer's own send before the daemon names it", () => {
    expect(nameSender(ACTOR.PENDING_STEER, FLEET, NAMES)).toBe(OWN_SENDER);
  });

  it("names a continuation after the person it resumed", () => {
    expect(nameSender(`continuation:${BOB}`, FLEET, NAMES)).toBe("Bob");
  });

  it("keeps the fallback for a member with no display name", () => {
    expect(nameSender(NAMELESS, FLEET, NAMES)).toBe(senderLabelFor(NAMELESS, FLEET));
    expect(namedMembers([member(NAMELESS, null)])).toEqual([]);
  });

  it("names no one before the viewer is known", () => {
    const anonymous = senderNamesFrom(null, []);
    expect(nameSender(`${ACTOR.STEER_PREFIX}${VIEWER}`, FLEET, anonymous)).toBe(
      senderLabelFor(`${ACTOR.STEER_PREFIX}${VIEWER}`, FLEET),
    );
    expect(nameSender(ACTOR.FLEET, FLEET, anonymous)).toBe(FLEET);
  });
});

describe("isNamedTeammate", () => {
  it("is true only for someone else the thread names", () => {
    expect(isNamedTeammate(BOB, NAMES)).toBe(true);
    expect(isNamedTeammate(`continuation:${BOB}`, NAMES)).toBe(true);
    expect(isNamedTeammate(`${ACTOR.STEER_PREFIX}${VIEWER}`, NAMES)).toBe(false);
    expect(isNamedTeammate(NAMELESS, NAMES)).toBe(false);
    expect(isNamedTeammate(STRANGER, NAMES)).toBe(false);
  });
});
