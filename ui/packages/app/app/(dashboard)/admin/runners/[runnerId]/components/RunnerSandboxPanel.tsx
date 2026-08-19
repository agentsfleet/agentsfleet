import { Badge, Card, Section, Time } from "@agentsfleet/design-system";
import { BIND_MODE, isSelftestStale, type ExtraBind, type RunnerDetail } from "@/lib/api/runners";

// What this runner's sandbox actually IS, and whether it has been proven.
// Both halves live in one panel deliberately: an operator reading a passing
// self-test needs to see the bind set it ran against, and a bind list with no
// verdict beside it is an assignment nobody has tested.

const PANEL_LABEL = "Sandbox";
const SELFTEST_HEADING = "Self-test";
const BINDS_HEADING = "Extra mounts";

const NEVER_TESTED = "Never self-tested. The verdict appears here once the host answers.";
const PENDING_NOTE = "A self-test is outstanding — the host answers on its next heartbeat.";
const STALE_BADGE = "stale";
const STALE_NOTE =
  "This verdict was recorded against an assignment the runner no longer carries, so it proves nothing about the current one.";
const ALL_OK_LABEL = "all checks passed";
const FAILED_SUFFIX = "failed";
const CHECK_YES = "✓";
const CHECK_NO = "✗";

const NO_BINDS = "Baseline only — no operator-assigned paths are mounted into this runner's leases.";
const BIND_MODE_SHORT: Record<string, string> = {
  read_only: "read-only",
  read_write: "read-write",
};

export function RunnerSandboxPanel({ runner }: { runner: RunnerDetail }) {
  // `?? null` throughout: a row written before these columns existed omits the
  // keys, so they arrive undefined and a strict null check would render an
  // Invalid Date or fall through to a missing verdict.
  const report = runner.selftest ?? null;
  const completedAt = runner.selftest_completed_at ?? null;
  const requestedAt = runner.selftest_requested_at ?? null;
  const stale = isSelftestStale(runner);
  const failed = report ? report.checks.filter((c) => !c.ok).length : 0;

  return (
    <Card className="flex flex-col gap-2xl p-lg" aria-label={PANEL_LABEL}>
      <Section className="flex flex-col gap-md">
        <div className="flex flex-wrap items-center gap-md">
          <h2 className="font-mono text-body-sm uppercase text-muted-foreground">{SELFTEST_HEADING}</h2>
          {report ? (
            <Badge variant={report.all_ok ? "green" : "error"}>
              {report.all_ok ? ALL_OK_LABEL : `${failed} ${FAILED_SUFFIX}`}
            </Badge>
          ) : null}
          {stale ? <Badge variant="amber">{STALE_BADGE}</Badge> : null}
          {completedAt !== null ? (
            <span className="text-body-sm text-muted-foreground">
              <Time value={new Date(completedAt)} format="relative" tooltip={false} />
            </span>
          ) : null}
        </div>

        {report === null ? <p className="text-body-sm text-muted-foreground">{NEVER_TESTED}</p> : null}
        {requestedAt !== null ? <p className="text-body-sm text-muted-foreground">{PENDING_NOTE}</p> : null}
        {stale ? <p className="font-mono text-body-sm text-warning">{STALE_NOTE}</p> : null}

        {report ? (
          <ul className="flex flex-col gap-sm">
            {report.checks.map((check) => (
              <li key={check.name} className="flex flex-wrap items-baseline gap-sm font-mono text-body-sm">
                <span aria-hidden="true" className={check.ok ? "text-success" : "text-destructive"}>
                  {check.ok ? CHECK_YES : CHECK_NO}
                </span>
                <span className="text-foreground">{check.name}</span>
                <span className="text-muted-foreground">{check.detail}</span>
              </li>
            ))}
          </ul>
        ) : null}
      </Section>

      <Section className="flex flex-col gap-md">
        <h2 className="font-mono text-body-sm uppercase text-muted-foreground">{BINDS_HEADING}</h2>
        <BindList binds={runner.assigned_policy?.extra_binds ?? []} />
      </Section>
    </Card>
  );
}

function BindList({ binds }: { binds: ExtraBind[] }) {
  if (binds.length === 0) return <p className="text-body-sm text-muted-foreground">{NO_BINDS}</p>;
  return (
    <ul className="flex flex-col gap-sm">
      {binds.map((bind) => {
        const mode = bind.mode ?? BIND_MODE.read_only;
        return (
          <li key={bind.path} className="flex flex-wrap items-baseline gap-sm font-mono text-body-sm">
            <span className="text-foreground">{bind.path}</span>
            {/* A writable mount is never reported quietly — tenant agent code
                can modify host state outside its workspace through it. */}
            <Badge variant={mode === BIND_MODE.read_write ? "amber" : "default"}>{BIND_MODE_SHORT[mode]}</Badge>
            {bind.note ? <span className="text-muted-foreground">{bind.note}</span> : null}
          </li>
        );
      })}
    </ul>
  );
}
