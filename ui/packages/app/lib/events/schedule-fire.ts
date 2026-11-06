import { z } from "zod";

const ScheduleFire = z.object({message: z.string(), schedule: z.object({
  schedule_id: z.string().min(1), description: z.string().min(1),
})});

/** Only the native cron envelope's display fields may enter its chat headline. */
export function scheduleFireHeadline(payload: unknown): string | null {
  const parsed = ScheduleFire.safeParse(payload);
  return parsed.success ? `Schedule ${parsed.data.schedule.description} fired: ${parsed.data.message}` : null;
}
