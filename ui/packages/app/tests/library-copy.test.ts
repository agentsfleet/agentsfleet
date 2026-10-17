import { expect, it } from "vitest";
import { libraryUnavailableDescription } from "@/lib/library-copy";

it.each([
  [401, false, "Your session has expired. Sign in again to continue."],
  [403, false, "Your account doesn’t have access to this library. Ask your administrator for access."],
  [404, false, "We couldn’t load the library. Please try again shortly."],
  [410, false, "We couldn’t load the library. Please try again shortly."],
  [404, true, "This published setup is no longer available, or your access has changed. Choose another setup."],
  [410, true, "This published setup is no longer available, or your access has changed. Choose another setup."],
  [503, true, "We couldn’t load the library. Please try again shortly."],
] as const)("gives the appropriate recovery for HTTP %i with selected recipe %s", (status, selected, message) => {
  expect(libraryUnavailableDescription(status, selected)).toBe(message);
});

it("handles an unknown read failure without exposing internal details", () => {
  expect(libraryUnavailableDescription()).toBe("We couldn’t load the library. Please try again shortly.");
});
