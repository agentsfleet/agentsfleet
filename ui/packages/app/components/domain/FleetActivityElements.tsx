"use client";

import type { ReactNode } from "react";
import type { ActivityDetails, ActivityElementData, FleetChatElementData } from "./fleet-chat-elements";
import Link from "next/link";
import { Button } from "@agentsfleet/design-system";
import { CheckCircleIcon, GitPullRequestIcon } from "@phosphor-icons/react/ssr";
import { senderLabelFor } from "@/lib/events/event-summary";
import { workspacePath } from "@/lib/workspace-routes";
import { FleetActivityRow, useSenderLabel } from "./FleetMessageRow";
import { FleetMarkdown } from "./FleetMarkdown";
import { FleetPayloadDisclosure } from "./FleetPayloadDisclosure";
import { FleetRunDetails } from "./FleetRunDetails";
import { useFleetScope } from "./FleetScope";

type VisibleActivityDetails = Exclude<ActivityDetails, {kind: "none"}>;

export function IntegrationActivityElement({data}: {data: ActivityElementData}) {
  return <ActivityRow data={data} annotation={data.source ? <SourceLink source={data.source} /> : null} />;
}

export function WakeupActivityElement({data}: {data: ActivityElementData}) {
  return <ActivityRow data={data} annotation={<FleetRunDetails eventId={data.eventId} status={data.status} />} />;
}

/** Navigation only: the inbox retains approval ownership and decision checks. */
export function ApprovalActivityElement({data}: {data: ActivityElementData}) {
  const scope = useFleetScope();
  const href = scope ? `${workspacePath(scope.workspaceId, "approvals")}?${new URLSearchParams({fleetId: scope.fleetId})}` : null;
  return <ActivityRow data={data} annotation={href ? <ActivityLink>
    <Link href={href}><CheckCircleIcon aria-hidden="true" />View approvals</Link>
  </ActivityLink> : null} />;
}

export function DefaultActivityElement({data}: {data: FleetChatElementData["default-activity"]}) {
  return <div><ActivityRow data={data} annotation={null} />
    {data.notice ? <p className="pb-lg text-label text-muted-foreground">{data.notice}</p> : null}
  </div>;
}

function ActivityRow({data, annotation}: {data: ActivityElementData; annotation: ReactNode}) {
  const senderLabel = useSenderLabel();
  return <FleetActivityRow sender={senderLabelFor(data.actor, senderLabel)} createdAt={data.createdAt}
    headline={data.headline} outcome={data.outcome} failed={data.failed} messageRole="system"
    detailsLabel={data.details.kind === "message" ? "View message" : "View event"} annotation={annotation}>
    {data.details.kind === "none" ? null : <ActivityContent details={data.details} />}
  </FleetActivityRow>;
}

function ActivityContent({details}: {details: VisibleActivityDetails}) {
  if (details.kind === "message") return <FleetMarkdown expandable>{details.text}</FleetMarkdown>;
  return <FleetPayloadDisclosure json={details.json} inline />;
}

function SourceLink({source}: {source: NonNullable<ActivityElementData["source"]>}) {
  return <ActivityLink><a href={source.href} target="_blank" rel="noreferrer noopener">
    {source.repositoryReference ? <GitPullRequestIcon aria-hidden="true" /> : null}{source.label}
  </a></ActivityLink>;
}

function ActivityLink({children}: {children: ReactNode}) {
  return <Button asChild variant="link" size="sm" className="h-auto min-h-11 gap-xs px-sm text-foreground underline decoration-border-strong underline-offset-2 hover:decoration-foreground sm:min-h-9">
    {children}
  </Button>;
}
