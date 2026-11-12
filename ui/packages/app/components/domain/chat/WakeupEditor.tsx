"use client";

import { useId } from "react";
import { Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, Input, Label, Textarea } from "@agentsfleet/design-system";
import type { ScheduleDetail, ScheduleEdit } from "@/lib/api/schedule-controls";

export function WakeupEditor({schedule, pending, error, onClose, onReload, onSave}: {
  schedule: ScheduleDetail; pending: boolean; error: string | null; onClose: () => void; onReload: () => void; onSave: (edit: ScheduleEdit) => void;
}) {
  const id = useId();
  return <Dialog open onOpenChange={() => {if (!pending) onClose();}}>
    <DialogContent closeDisabled={pending} className="ph-no-capture ph-no-record"><DialogHeader><DialogTitle>Edit wake-up</DialogTitle>
      <DialogDescription>Change when this fleet wakes and what it should check.</DialogDescription></DialogHeader>
      <form key={schedule.etag} className="space-y-xl" onSubmit={event => {
        event.preventDefault(); const form = new FormData(event.currentTarget);
        onSave({cron: field(form, "cron").trim(), timezone: field(form, "timezone").trim(), message: field(form, "message")});
      }}>
        <div className="space-y-md"><Label htmlFor={`${id}-cron`}>Schedule</Label><Input id={`${id}-cron`} name="cron" required maxLength={256} defaultValue={schedule.cron} disabled={pending} />
          <p className="text-eyebrow text-text-dim">Five-field cron expression. Current schedule: {schedule.description}</p></div>
        <div className="space-y-md"><Label htmlFor={`${id}-zone`}>Timezone</Label><Input id={`${id}-zone`} name="timezone" required maxLength={256} defaultValue={schedule.timezone} disabled={pending} /></div>
        <div className="space-y-md"><Label htmlFor={`${id}-message`}>What should the fleet do?</Label><Textarea id={`${id}-message`} name="message" required defaultValue={schedule.message} disabled={pending} /></div>
        {error ? <div role="alert" className="text-label text-destructive"><p>{error}</p><Button type="button" variant="link" disabled={pending} onClick={onReload}>Reload saved schedule</Button></div> : null}
        <div className="flex justify-end gap-md"><Button type="button" variant="outline" disabled={pending} onClick={onClose}>Cancel</Button><Button type="submit" disabled={pending}>{pending ? "Saving…" : "Save"}</Button></div>
      </form>
    </DialogContent>
  </Dialog>;
}

function field(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === "string" ? value : "";
}
