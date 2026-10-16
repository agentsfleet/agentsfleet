import Link from "next/link";
import { ArrowUpRightIcon, PlusIcon } from "lucide-react";
import { BrandMark, Button, PageTitle } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";

export function NewFleetEntrance({ workspaceId }: { workspaceId: string }) {
  const href = workspacePath(workspaceId, "fleets/new");
  return <div className="fleet-entrance flex flex-col items-center gap-xl py-2xl text-center sm:py-4xl">
    <BrandMark className="size-10 text-pulse" aria-hidden="true" />
    <div className="space-y-md">
      <PageTitle>What should your fleet do?</PageTitle>
      <p className="mx-auto max-w-prose text-body-lg text-muted-foreground">Give it a job. Shape the plan together. Put it to work.</p>
    </div>
    <Button asChild size="lg"><Link href={href}><PlusIcon size={18} />New fleet<ArrowUpRightIcon size={16} /></Link></Button>
    <p className="text-label text-muted-foreground">Start with a conversation. You approve the setup.</p>
  </div>;
}
