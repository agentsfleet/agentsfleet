import type { ExampleJob } from "./example-jobs";

export const STORY_ROLE = { user: "user", fleet: "fleet", event: "event" } as const;
export const STORY_DETAIL = { evidence: "evidence", proposal: "proposal" } as const;
export const STORY_FRAME_MS = 50;
const CHARACTER_MS = 16;
const REPLY_WAIT_MS = 1100;
const USER_WAIT_MS = 450;
const READING_MS = 1700;
const END_HOLD_MS = 6500;

export type StoryStep = {
  role: (typeof STORY_ROLE)[keyof typeof STORY_ROLE];
  text: string;
  activity?: string;
  detail?: (typeof STORY_DETAIL)[keyof typeof STORY_DETAIL];
};
export type StoryEntry = { step: StoryStep; text: string; complete: boolean };
export type StoryFrame = { entries: StoryEntry[]; pending: StoryStep | null; finished: boolean; cycle: number };
type TimedStep = { step: StoryStep; startsAt: number; typesAt: number; endsAt: number };
export type ChatStory = { steps: TimedStep[]; duration: number };

export function createChatStory(job: ExampleJob): ChatStory {
  const steps: StoryStep[] = [
    { role: STORY_ROLE.user, text: job.request },
    { role: STORY_ROLE.fleet, activity: "Understanding the job", text: job.question },
    { role: STORY_ROLE.user, text: job.setupReply },
    { role: STORY_ROLE.fleet, activity: "Preparing the setup", text: job.setupReview },
    { role: STORY_ROLE.user, text: "Save this setup and create the fleet." },
    { role: STORY_ROLE.event, text: `${job.name} created. Waiting for its next scheduled check.` },
    { role: STORY_ROLE.event, text: job.check },
    { role: STORY_ROLE.fleet, activity: job.activity, text: job.investigation, detail: STORY_DETAIL.evidence },
    { role: STORY_ROLE.fleet, activity: "Tracing the cause", text: job.finding },
    { role: STORY_ROLE.fleet, activity: "Preparing a recommendation", text: job.proposal, detail: STORY_DETAIL.proposal },
    { role: STORY_ROLE.fleet, activity: "Checking the access boundary", text: job.boundary },
    { role: STORY_ROLE.fleet, activity: "Writing the report", text: job.report },
  ];
  let cursor = 0;
  const timed = steps.map((step) => {
    const startsAt = cursor;
    const typesAt = startsAt + (step.role === STORY_ROLE.fleet ? REPLY_WAIT_MS : step.role === STORY_ROLE.user ? USER_WAIT_MS : 0);
    const endsAt = typesAt + (step.role === STORY_ROLE.event ? 0 : step.text.length * CHARACTER_MS);
    cursor = endsAt + READING_MS;
    return { step, startsAt, typesAt, endsAt };
  });
  return { steps: timed, duration: cursor + END_HOLD_MS };
}

export function storyFrame(story: ChatStory, elapsed: number, complete = false): StoryFrame {
  const cycle = Math.floor(elapsed / story.duration);
  const time = elapsed % story.duration;
  const entries: StoryEntry[] = [];
  let pending: StoryStep | null = null;
  for (const item of story.steps) {
    if (!complete && time < item.typesAt) {
      if (time >= item.startsAt && item.step.role === STORY_ROLE.fleet) pending = item.step;
      break;
    }
    const done = complete || time >= item.endsAt;
    const characters = done ? item.step.text.length : Math.floor((time - item.typesAt) / CHARACTER_MS);
    entries.push({ step: item.step, text: item.step.text.slice(0, characters), complete: done });
    if (!done) break;
  }
  return { entries, pending, cycle, finished: entries.length === story.steps.length && entries.every((entry) => entry.complete) };
}
