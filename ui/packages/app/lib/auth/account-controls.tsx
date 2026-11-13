"use client";

import { UserProfile } from "@clerk/nextjs";
import { PROFILE_APPEARANCE } from "@/lib/profile-appearance";

/** Native verification retains hash routing on the combined account page. */
export function AccountControls() {
  return <UserProfile routing="hash" appearance={PROFILE_APPEARANCE} />;
}
