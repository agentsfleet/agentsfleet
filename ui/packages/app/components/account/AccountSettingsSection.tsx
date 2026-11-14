"use client";

import * as Avatar from "@radix-ui/react-avatar";
import { DashboardPanel, Section } from "@agentsfleet/design-system";
import { UserIcon } from "@phosphor-icons/react/ssr";
import { useCurrentUser } from "@/lib/auth/client";
import { AccountAccess } from "./AccountAccess";

export function AccountSettingsSection() {
  const user = useCurrentUser();
  return <Section aria-label="Your account">
    {!user.isLoaded || !user.isSignedIn ? <AccountAccess status={user.isLoaded ? "signed-out" : "loading"} /> :
      <DashboardPanel className="flex min-w-0 items-center gap-xl">
        <Avatar.Root key={user.userId} className="flex size-12 shrink-0 items-center justify-center overflow-hidden rounded-full bg-muted" aria-hidden="true">
          <Avatar.Image src={user.imageUrl ?? undefined} alt="" className="size-full object-cover" />
          <Avatar.Fallback><UserIcon size={24} /></Avatar.Fallback>
        </Avatar.Root>
        <div className="min-w-0 space-y-xs">
          <p className="text-eyebrow text-muted-foreground">Signed in as</p>
          <h2 className="break-words text-body font-semibold">{user.displayName || "Your account"}</h2>
          <p className="break-all text-label text-muted-foreground">{user.emailAddress || "No primary email address"}</p>
        </div>
      </DashboardPanel>}
  </Section>;
}
