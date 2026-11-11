"use client";

import { UserProfile } from "@clerk/nextjs";
import { PROFILE_APPEARANCE } from "@/lib/profile-appearance";

/** A page keeps its name when native verification switches panels. */
export function AccountControls() {
  return <UserProfile routing="hash" appearance={PROFILE_APPEARANCE} />;
}
