"use client";

import { Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from "@agentsfleet/design-system";
import { AccountMethodsSettings } from "./AccountMethodsSettings";

/** Credential verification stays inside the authenticated provider facade. */
export function ManageAccountDialog() {
  return <Dialog>
    <DialogTrigger asChild><Button variant="secondary">Manage account and sign-in</Button></DialogTrigger>
    <DialogContent>
      <DialogHeader><DialogTitle>Account and sign-in</DialogTitle>
        <DialogDescription>Manage your profile, email addresses and sign-in methods.</DialogDescription>
      </DialogHeader>
      <AccountMethodsSettings />
    </DialogContent>
  </Dialog>;
}
