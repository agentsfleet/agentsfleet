"use client";

import dynamic from "next/dynamic";

// Keep the conversation visible while a scheduled fleet loads its Summary.
export const ScheduledSetupSummary = dynamic(
  () => import("@/components/domain/setup/summary/SetupSummary").then(module => module.SetupSummary),
  {loading: () => null},
);
