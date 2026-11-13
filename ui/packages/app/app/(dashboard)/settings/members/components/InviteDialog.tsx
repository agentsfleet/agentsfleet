"use client";

import { useState } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
  ActionForm,
  Alert,
  Button,
  CopyButton,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  Spinner,
  Time,
} from "@agentsfleet/design-system";
import { PlusIcon, UsersIcon } from "@phosphor-icons/react/ssr";
import type { InviteSummary } from "@/lib/api/invites";
import { EMAIL_STATUS, type EmailStatus } from "@/lib/api/invites-types";
import { presentErrorString } from "@/lib/errors";
import { createInviteAction } from "../actions";

// The backend lowercases and checks the address itself; this only spares a
// round trip for something that is plainly not one.
const ENTER_AN_ADDRESS = "Enter an email address";
// RFC 5321 caps the part before the @ at 64 characters, and the backend's mail
// parser refuses a longer one; zod's email rule alone lets it through.
const LOCAL_PART_MAX = 64;
const schema = z.object({
  email: z
    .string()
    .trim()
    .pipe(z.email(ENTER_AN_ADDRESS))
    .refine((email) => email.lastIndexOf("@") <= LOCAL_PART_MAX, ENTER_AN_ADDRESS),
});
type FormValues = z.infer<typeof schema>;

/** `onSettled` runs after every create attempt: a refused or timed-out create
 * may still have saved the invite, so the list re-reads either way. */
export default function InviteDialog({ onSettled }: { onSettled: () => void }) {
  const [open, setOpen] = useState(false);
  const [created, setCreated] = useState<InviteSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  // True only while the create is in flight. Not a transition: React would
  // entangle it with the page's list reload that `onSettled` starts, and the
  // dialog, even one reopened for the next invite, would wait on that reload.
  const [sending, setSending] = useState(false);

  // Closing from any path starts the next invite afresh; the form itself
  // unmounts with the dialog. A send in flight finishes here first, or its
  // answer would land in the next invite.
  function handleOpenChange(next: boolean) {
    if (!next && sending) return;
    setOpen(next);
    if (next) return;
    setCreated(null);
    setError(null);
  }

  async function send(email: string) {
    setError(null);
    setSending(true);
    let result: Awaited<ReturnType<typeof createInviteAction>>;
    try {
      result = await createInviteAction(email);
    } finally {
      setSending(false);
    }
    onSettled();
    if (!result.ok) {
      setError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: "create the invite" }));
      return;
    }
    setCreated(result.data);
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <Button type="button" size="sm">
          <PlusIcon size={14} />
          Invite member
        </Button>
      </DialogTrigger>
      <DialogContent closeDisabled={sending}>
        {created ? (
          <InviteReady invite={created} onDone={() => handleOpenChange(false)} />
        ) : (
          <InviteForm error={error} pending={sending} onSend={send} onCancel={() => handleOpenChange(false)} />
        )}
      </DialogContent>
    </Dialog>
  );
}

type InviteFormProps = {
  error: string | null;
  pending: boolean;
  onSend: (email: string) => Promise<void>;
  onCancel: () => void;
};

// The address to invite. It mounts each time the dialog opens, so every invite
// starts from an empty field.
function InviteForm({ error, pending, onSend, onCancel }: InviteFormProps) {
  const form = useForm<FormValues>({ resolver: zodResolver(schema), defaultValues: { email: "" } });
  return (
    <>
      <DialogHeader>
        <DialogTitle>Invite a teammate</DialogTitle>
        <DialogDescription>They can open every workspace in your account.</DialogDescription>
      </DialogHeader>
      <Form {...form}>
        {/* The schema's message, not the browser's bubble, explains a bad address. */}
        <ActionForm className="space-y-0" onSubmit={(e) => { void form.handleSubmit((values) => onSend(values.email))(e); }} noValidate>
          <div className="space-y-4">
            <FormField
              control={form.control}
              name="email"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Email address</FormLabel>
                  <FormControl>
                    <Input type="email" placeholder="teammate@example.com" autoComplete="off" {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
            <div className="flex items-start gap-lg rounded-md border border-border p-xl"><UsersIcon size={20} className="mt-xs shrink-0 text-muted-foreground" aria-hidden="true" /><p className="text-label text-muted-foreground">They’ll join as a <strong className="font-semibold text-foreground">Member</strong> and can open <strong className="font-semibold text-foreground">every workspace in this account</strong>.</p></div>
            {error ? <Alert variant="destructive">{error}</Alert> : null}
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" disabled={pending} onClick={onCancel}>
              Cancel
            </Button>
            <Button type="submit" disabled={pending}>
              {pending ? <Spinner size="sm" srLabel="Sending" /> : null}
              Send invite
            </Button>
          </DialogFooter>
        </ActionForm>
      </Form>
    </>
  );
}

const INVITE_CREATED = "Invite created";

// Only a relay that accepted the email earns "sent". An invite whose email
// failed, or that has no relay to send it, still exists, and its link is the
// way in.
const READY_COPY: Record<EmailStatus, { title: string; lead: (email: string) => string }> = {
  [EMAIL_STATUS.sent]: {
    title: "Invite sent",
    lead: (email) => `We emailed ${email}. You can also copy the link and share it.`,
  },
  [EMAIL_STATUS.failed]: {
    title: INVITE_CREATED,
    lead: (email) => `The email to ${email} did not go out. Copy the link and share it, or send the email again from the list.`,
  },
  [EMAIL_STATUS.unconfigured]: {
    title: INVITE_CREATED,
    lead: (email) => `This deployment sends no email. Copy the link and share it with ${email}.`,
  },
};

// The link shows the moment the invite exists. The table keeps a copy action
// on the row, so closing loses nothing.
function InviteReady({ invite, onDone }: { invite: InviteSummary; onDone: () => void }) {
  const copy = READY_COPY[invite.email_status];
  return (
    <div data-testid="invite-ready">
      <DialogHeader>
        <DialogTitle>{copy.title}</DialogTitle>
        <DialogDescription>
          {copy.lead(invite.email)} It expires <Time value={new Date(invite.expires_at)} format="relative" />.
        </DialogDescription>
      </DialogHeader>
      <div className="flex items-center gap-sm">
        <Input readOnly value={invite.link} aria-label="Invite link" onFocus={(e) => e.currentTarget.select()} />
        <CopyButton value={invite.link} label="Copy invite link" />
      </div>
      <DialogFooter>
        <Button type="button" onClick={onDone}>Done</Button>
      </DialogFooter>
    </div>
  );
}
