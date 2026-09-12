"use client";

import { useState } from "react";
import { ShieldAlertIcon, ShieldCheckIcon, ShieldIcon, type LucideIcon } from "lucide-react";
import {
  Button,
  cn,
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

type Verdict = { text: string; Icon: LucideIcon; className: string };

// The verdict, compressed to a word the identity line can carry: passed,
// failed with a count, stale, pending, or never. A failure is the one state
// that must be seen without a click, so it is the one state painted red; the
// full report — every check by name, the mounts it ran against — opens in a
// dialog from here rather than taking a card on the page.
function verdictFor(runner: RunnerDetail): Verdict {
  const report = runner.selftest ?? null;
  if (report === null) {
    return {
      text: (runner.selftest_requested_at ?? null) !== null ? CHECKS_BADGE_PENDING : CHECKS_BADGE_NEVER,
      Icon: ShieldIcon,
      className: "text-muted-foreground",
    };
  }
  if (isSelftestStale(runner)) {
    return { text: CHECKS_BADGE_STALE, Icon: ShieldAlertIcon, className: "text-warning" };
  }
  if (report.all_ok) {
    return { text: CHECKS_BADGE_PASSED, Icon: ShieldCheckIcon, className: "text-success" };
  }
  const failed = report.checks.filter((check) => !check.ok).length;
  return {
    text: `${failed} ${CHECKS_BADGE_FAILED_SUFFIX}`,
    Icon: ShieldAlertIcon,
    className: "text-destructive",
  };
}

export function RunnerChecksBadge({ runner }: { runner: RunnerDetail }) {
  const [open, setOpen] = useState(false);
  const verdict = verdictFor(runner);
  const completedAt = runner.selftest_completed_at ?? null;
  return (
    <>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(true)}
        className={cn("h-auto gap-sm px-sm py-xs font-sans text-label uppercase tracking-label", verdict.className)}
      >
        <verdict.Icon size={ICON_SIZE} aria-hidden="true" />
        {verdict.text}
        {completedAt !== null ? (
          <Time
            value={new Date(completedAt)}
            format="relative"
            tooltip={false}
            className="normal-case text-muted-foreground"
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
