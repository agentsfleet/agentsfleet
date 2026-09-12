"use client";

import { useState } from "react";
import { ShieldAlertIcon, ShieldCheckIcon, ShieldIcon, type LucideIcon } from "lucide-react";
import type { BadgeVariant } from "@agentsfleet/design-system";
import {
  badgeVariants,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Time,
} from "@agentsfleet/design-system";
import { isSelftestStale, type RunnerDetail } from "@/lib/api/runners";
import { RunnerChecksReport } from "./RunnerChecksReport";
import {
  CHECKS_BADGE_FAILED_SUFFIX,
  CHECKS_BADGE_NEVER,
  CHECKS_BADGE_PASSED,
  CHECKS_BADGE_PENDING,
  CHECKS_BADGE_STALE,
  CHECKS_DIALOG_DESCRIPTION,
  CHECKS_DIALOG_TITLE,
} from "./runner-copy";

const ICON_SIZE = 13;

type Verdict = { text: string; Icon: LucideIcon; variant: BadgeVariant };

// The verdict, compressed to a word the identity line can carry: passed,
// failed with a count, stale, pending, or never. It wears the Badge recipe so
// it reads as one more pill beside the tier and labels, not as a stray link;
// a failure is the one state that must be seen without a click, so it is the
// one state painted red. The full report — every check by name, the mounts
// it ran against — opens in a dialog from here rather than taking a card on
// the page.
function verdictFor(runner: RunnerDetail): Verdict {
  const report = runner.selftest ?? null;
  if (report === null) {
    return {
      text: (runner.selftest_requested_at ?? null) !== null ? CHECKS_BADGE_PENDING : CHECKS_BADGE_NEVER,
      Icon: ShieldIcon,
      variant: "default",
    };
  }
  if (isSelftestStale(runner)) {
    return { text: CHECKS_BADGE_STALE, Icon: ShieldAlertIcon, variant: "amber" };
  }
  if (report.all_ok) {
    return { text: CHECKS_BADGE_PASSED, Icon: ShieldCheckIcon, variant: "green" };
  }
  const failed = report.checks.filter((check) => !check.ok).length;
  return {
    text: `${failed} ${CHECKS_BADGE_FAILED_SUFFIX}`,
    Icon: ShieldAlertIcon,
    variant: "destructive",
  };
}

export function RunnerChecksBadge({ runner }: { runner: RunnerDetail }) {
  const [open, setOpen] = useState(false);
  const verdict = verdictFor(runner);
  const completedAt = runner.selftest_completed_at ?? null;
  return (
    <>
      {/* The eyebrow size is geometry only: no fill, no padding of its own, so
          the pill inside is the whole visible control and the button is its
          hit area and focus ring. */}
      <Button
        type="button"
        variant="ghost"
        size="eyebrow"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(true)}
        className="h-auto gap-sm px-0 hover:bg-transparent"
      >
        <span className={badgeVariants({ variant: verdict.variant })}>
          <verdict.Icon size={ICON_SIZE} aria-hidden="true" />
          {verdict.text}
        </span>
        {completedAt !== null ? (
          <Time
            value={new Date(completedAt)}
            format="relative"
            tooltip={false}
            className="font-sans text-label text-muted-foreground"
          />
        ) : null}
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{CHECKS_DIALOG_TITLE}</DialogTitle>
            <DialogDescription className="font-sans">{CHECKS_DIALOG_DESCRIPTION}</DialogDescription>
          </DialogHeader>
          <RunnerChecksReport runner={runner} />
        </DialogContent>
      </Dialog>
    </>
  );
}
