"use client";

import { Alert, Button, DashboardPanel, Skeleton, Spinner } from "@agentsfleet/design-system";
import { useAuthLoadFailed } from "@/lib/auth/client";

type Props = {kind: "sign-in" | "sign-up"};

export function AuthFormLoading({kind}: Props) {
  const failed = useAuthLoadFailed();
  return <DashboardPanel className="w-full max-w-auth-card space-y-2xl rounded-lg p-2xl" data-auth-form-loading="">
    {failed ? <>
      <Alert variant="destructive">Couldn’t load {kind}. Reload the page to try again.</Alert>
      <Button type="button" variant="outline" size="sm" onClick={() => window.location.reload()}>Reload page</Button>
    </> : <>
      <Spinner size="sm" label={`Loading ${kind}…`} />
      <div aria-hidden="true" className="space-y-xl motion-reduce:[&_*]:animate-none">
        <Skeleton className="h-11 rounded-control" />
        <div className="space-y-sm">
          <Skeleton className="h-4 w-24" />
          <Skeleton className="h-11 rounded-control" />
        </div>
        <Skeleton className="h-11 rounded-control" />
      </div>
    </>}
  </DashboardPanel>;
}
