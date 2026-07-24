import Link from "next/link";
import { notFound, redirect } from "next/navigation";
import {
  Badge,
  Card,
  CardContent,
  CopyButton,
  DescriptionList,
  DescriptionTerm,
  DescriptionDetails,
  PageHeader,
  PageLayout,
  PageTitle,
  Section,
  SectionLabel,
  Time,
} from "@agentsfleet/design-system";

import { auth } from "@clerk/nextjs/server";
import { getApproval, type ApprovalGate } from "@/lib/api/approvals";
import { workspacePath } from "@/lib/workspace-routes";
import ResolveButtons from "./ResolveButtons";

export const dynamic = "force-dynamic";

export default async function ApprovalDetailPage({
  params,
}: {
  params: Promise<{ workspaceId: string; gateId: string }>;
}) {
  const { workspaceId, gateId } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const gate = await getApproval(workspaceId, gateId, token).catch(() => null);
  if (!gate) notFound();

  const terminal = gate.status !== "pending";
  const statusVariant: "default" | "destructive" | "green" | "amber" =
    gate.status === "approved" ? "green"
    : gate.status === "denied" ? "destructive"
    : gate.status === "timed_out" || gate.status === "auto_killed" ? "amber"
    : "default";

  return (
    <PageLayout>
      <PageHeader>
        <div className="flex items-center gap-3">
          <PageTitle>{gate.proposed_action || `${gate.tool_name}:${gate.action_name}`}</PageTitle>
          <Badge variant={statusVariant}>{gate.status}</Badge>
        </div>
      </PageHeader>

      <Section asChild>
        <section aria-label="Gate metadata">
          <SectionLabel>Context</SectionLabel>
          <Card>
            <CardContent className="pt-6">
              <KeyValueGrid gate={gate} workspaceId={workspaceId} />
            </CardContent>
          </Card>
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Evidence">
          <SectionLabel>Evidence</SectionLabel>
          <Card>
            <CardContent className="pt-6">
              <pre className="overflow-x-auto whitespace-pre-wrap break-words rounded-md bg-muted p-4 text-xs">
                {JSON.stringify(gate.evidence, null, 2)}
              </pre>
            </CardContent>
          </Card>
        </section>
      </Section>

      {terminal ? (
        <Section asChild>
          <section aria-label="Resolution">
            <SectionLabel>Resolution</SectionLabel>
            <Card>
              <CardContent className="pt-6 text-sm">
                Resolved as <strong>{gate.status}</strong> by {gate.resolved_by || "(unknown)"}
                {gate.updated_at ? (
                  <>
                    {" at "}
                    <Time value={new Date(gate.updated_at)} tooltip={false} />
                  </>
                ) : null}.
                {gate.detail ? <> Reason: <em>{gate.detail}</em></> : null}
              </CardContent>
            </Card>
          </section>
        </Section>
      ) : (
        <Section asChild>
          <section aria-label="Resolve">
            <SectionLabel>Resolve</SectionLabel>
            <Card>
              <CardContent className="pt-6">
                <ResolveButtons workspaceId={workspaceId} gateId={gate.gate_id} />
              </CardContent>
            </Card>
          </section>
        </Section>
      )}
    </PageLayout>
  );
}

function KeyValueGrid({ gate, workspaceId }: { gate: ApprovalGate; workspaceId: string }) {
  return (
    <DescriptionList
      layout="stacked"
      className="grid grid-cols-1 gap-y-3 text-sm sm:grid-cols-[max-content_1fr] sm:gap-x-6 space-y-0"
    >
      <Row label="Fleet" value={
        <Link href={workspacePath(workspaceId, `fleets/${gate.fleet_id}`)} className="hover:underline">
          {gate.fleet_name}
        </Link>
      } />
      <Row label="Tool" value={<code className="font-mono text-xs">{gate.tool_name}</code>} />
      <Row label="Action" value={<code className="font-mono text-xs">{gate.action_name}</code>} />
      {gate.gate_kind ? <Row label="Kind" value={<Badge variant="default">{gate.gate_kind}</Badge>} /> : null}
      {gate.blast_radius ? <Row label="Blast radius" value={gate.blast_radius} /> : null}
      <Row label="Requested" value={<Time value={new Date(gate.requested_at)} tooltip={false} />} />
      <Row label="Auto-deny at" value={<Time value={new Date(gate.timeout_at)} tooltip={false} />} />
      {/* An action id exists only to be quoted back — in a support thread, a log
          grep, an API call. It is the one field on this page nobody reads, they
          only copy it. */}
      <Row
        label="Action id"
        value={
          <span className="flex items-center gap-1">
            <code className="font-mono text-xs">{gate.action_id}</code>
            <CopyButton value={gate.action_id} label="Copy action id" />
          </span>
        }
      />
    </DescriptionList>
  );
}

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <>
      <DescriptionTerm className="font-medium">{label}</DescriptionTerm>
      <DescriptionDetails>{value}</DescriptionDetails>
    </>
  );
}
