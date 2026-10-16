"use client";

import { useState } from "react";
import { ShieldAlertIcon, ShieldCheckIcon, ShieldIcon, ShieldXIcon, type LucideIcon } from "lucide-react";
import { TooltipButton, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, Time } from "@agentsfleet/design-system";
import type { RunnerDetail } from "@/lib/api/runners";
import { isSelftestStale } from "@/lib/api/runners-types";
import { RunnerChecksReport } from "./RunnerChecksReport";
import {
  CHECKS_BADGE_FAILED_SUFFIX, CHECKS_BADGE_NEVER, CHECKS_BADGE_PASSED,
  CHECKS_BADGE_PENDING, CHECKS_BADGE_STALE, CHECKS_DIALOG_DESCRIPTION,
} from "./runner-copy";

type Verdict = { text: string; Icon: LucideIcon; tone: string };

function verdictFor(runner: RunnerDetail): Verdict {
  const report = runner.selftest;
  if (report === null) {
    return {
      text: runner.selftest_requested_at !== null ? CHECKS_BADGE_PENDING : CHECKS_BADGE_NEVER,
      Icon: ShieldIcon,
      tone: "text-muted-foreground",
    };
  }
  if (isSelftestStale(runner)) {
    return { text: CHECKS_BADGE_STALE, Icon: ShieldAlertIcon, tone: "text-warning" };
  }
  if (report.all_ok) {
    return { text: CHECKS_BADGE_PASSED, Icon: ShieldCheckIcon, tone: "text-success" };
  }
  const failed = report.checks.filter((check) => !check.ok).length;
  return { text: `${failed} ${CHECKS_BADGE_FAILED_SUFFIX}`, Icon: ShieldXIcon, tone: "text-destructive" };
}

export function RunnerChecksBadge({ runner }: { runner: RunnerDetail }) {
  const [open, setOpen] = useState(false);
  const verdict = verdictFor(runner);
  const label = `${verdict.text}. Open report`;
  return (
    <>
      <TooltipButton type="button" variant="ghost" size="icon-sm" className={`shrink-0 ${verdict.tone}`}
        tooltip={<VerdictTooltip label={label} completedAt={runner.selftest_completed_at} />}
        aria-label={label} aria-haspopup="dialog" aria-expanded={open} onClick={() => setOpen(true)}>
        <verdict.Icon className="size-5" aria-hidden="true" />
      </TooltipButton>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="break-all pr-xl"><code className="font-mono">{runner.host_id}</code></DialogTitle>
            <DialogDescription>{CHECKS_DIALOG_DESCRIPTION}</DialogDescription>
          </DialogHeader>
          <RunnerChecksReport runner={runner} />
        </DialogContent>
      </Dialog>
    </>
  );
}

function VerdictTooltip({ label, completedAt }: { label: string; completedAt: number | null }) {
  const completed = completedAt === null ? null : new Date(completedAt);
  return <div className="flex flex-col gap-xs">
    <p className="font-medium">{label}</p>
    {completed ? <>
      <p>Last checked <Time value={completed} format="relative" tooltip={false} /></p>
      <Time value={completed} format="absolute" tooltip={false} />
    </> : null}
  </div>;
}
