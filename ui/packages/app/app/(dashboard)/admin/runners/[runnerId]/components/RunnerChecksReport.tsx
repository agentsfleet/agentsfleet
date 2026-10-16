import { Badge, List, ListItem, Section, Separator } from "@agentsfleet/design-system";
import type { RunnerDetail } from "@/lib/api/runners";
import { BIND_MODE, isSelftestStale, type ExtraBind } from "@/lib/api/runners-types";

const REPORT_TEST_ID = "runner-checks-report";
const BINDS_HEADING = "Extra mounts";

const NEVER_TESTED = "Never checked. The verdict appears here once the host answers.";
const PENDING_NOTE = "Checks are outstanding — the host answers on its next heartbeat.";
const STALE_BADGE = "stale";
const STALE_NOTE =
  "This verdict was recorded against an assignment the runner no longer carries, so it proves nothing about the current one.";
const ALL_OK_LABEL = "all checks passed";
const FAILED_SUFFIX = "failed";
const SUCCESS_DETAIL = "no fault detected";
const CHECK_YES = "✓";
const CHECK_NO = "✗";

const NO_BINDS = "No additional mounts.";
const BIND_MODE_SHORT: Record<string, string> = {
  read_only: "read-only",
  read_write: "read-write",
};

export function RunnerChecksReport({ runner }: { runner: RunnerDetail }) {
  const report = runner.selftest;
  const requestedAt = runner.selftest_requested_at;
  const stale = isSelftestStale(runner);
  const failed = report ? report.checks.filter((c) => !c.ok).length : 0;

  return (
    <Section data-testid={REPORT_TEST_ID} className="flex flex-col gap-2xl">
      <Section className="flex flex-col gap-md">
        <div className="flex flex-wrap items-center gap-md">
          {report ? (
            <Badge variant={report.all_ok ? "green" : "error"}>
              {report.all_ok ? ALL_OK_LABEL : `${failed} ${FAILED_SUFFIX}`}
            </Badge>
          ) : null}
          {stale ? <Badge variant="amber">{STALE_BADGE}</Badge> : null}
        </div>

        {report === null ? <p className="text-body-sm text-muted-foreground">{NEVER_TESTED}</p> : null}
        {requestedAt !== null ? <p className="text-body-sm text-muted-foreground">{PENDING_NOTE}</p> : null}
        {stale ? <p className="font-sans text-body-sm text-warning">{STALE_NOTE}</p> : null}

        {report ? (
          <List variant="plain" className="space-y-lg">
            {report.checks.map((check) => (
              <ListItem key={check.name} className="flex items-start gap-md text-body-sm leading-body-sm">
                <span aria-hidden="true" className={`shrink-0 ${check.ok ? "text-success" : "text-destructive"}`}>
                  {check.ok ? CHECK_YES : CHECK_NO}
                </span>
                <div className="min-w-0">
                  <p className="font-medium"><span className="sr-only">{check.ok ? "Passed: " : "Failed: "}</span>{check.name}</p>
                  {check.detail !== SUCCESS_DETAIL ? <p className="mt-xs text-muted-foreground">{check.detail}</p> : null}
                </div>
              </ListItem>
            ))}
          </List>
        ) : null}
      </Section>

      <Separator />
      <Section className="flex flex-col gap-sm">
        <h2 className="font-sans text-body-sm font-medium">{BINDS_HEADING}</h2>
        <BindList binds={runner.assigned_policy.extra_binds} />
      </Section>
    </Section>
  );
}

function BindList({ binds }: { binds: ExtraBind[] }) {
  if (binds.length === 0) return <p className="text-body-sm text-muted-foreground">{NO_BINDS}</p>;
  return (
    <List variant="plain" className="space-y-sm">
      {binds.map((bind) => (
        <ListItem key={bind.path} className="flex flex-wrap items-baseline gap-sm font-mono text-mono leading-mono">
          <span className="text-foreground">{bind.path}</span>
          {/* A writable mount is never reported quietly — tenant agent code
              can modify host state outside its workspace through it. */}
          <Badge variant={bind.mode === BIND_MODE.read_write ? "amber" : "default"}>{BIND_MODE_SHORT[bind.mode]}</Badge>
          {bind.note ? <span className="text-muted-foreground">{bind.note}</span> : null}
        </ListItem>
      ))}
    </List>
  );
}
