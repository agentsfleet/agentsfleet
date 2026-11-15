"use client";

import * as Avatar from "@radix-ui/react-avatar";
import { Spinner } from "@agentsfleet/design-system";
import { UserCircleIcon } from "@phosphor-icons/react/ssr";

type Props = { userId: string | null; name: string | null | undefined; imageUrl: string | null; busy?: boolean };

/** Account images share the rail's initials fallback and reset with the person. */
export function AccountAvatar({userId, name, imageUrl, busy = false}: Props) {
  const initials = name?.trim().split(/\s+/).slice(0, 2).map(part => part[0]).join("").toUpperCase();
  return <Avatar.Root key={userId} data-account-avatar="" aria-hidden={busy ? undefined : true}
    className="flex size-8 shrink-0 items-center justify-center overflow-hidden rounded-full bg-avatar text-avatar-foreground">
    {busy ? <Spinner size="sm" srLabel="Loading workspace menu" /> : <>
      <Avatar.Image src={imageUrl ?? undefined} alt="" className="size-full object-cover" />
      <Avatar.Fallback className="text-label font-extrabold">{initials || <UserCircleIcon size={18} />}</Avatar.Fallback>
    </>}
  </Avatar.Root>;
}
