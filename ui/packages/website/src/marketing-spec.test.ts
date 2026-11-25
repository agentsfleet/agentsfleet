import { expect, it } from "vitest";
import { createChatStory } from "./lib/chat-story";
import { EXAMPLE_JOBS } from "./lib/example-jobs";

it.each(EXAMPLE_JOBS)("opens $name from the first message and approves access before the check", (job) => {
  const steps = createChatStory(job).steps.map(({ step }) => step.text);
  expect(steps.slice(0, 2)).toEqual([job.request, job.firstReply]);
  expect(steps.slice(2, 2 + job.approvedAccess.length)).toEqual(job.approvedAccess.map((access) => `Access approved · ${access}`));
  expect(steps.indexOf(`Schedule added · ${job.schedule}`)).toBeLessThan(steps.indexOf(job.check));
  expect(steps.join(" ")).not.toMatch(/save this setup|before saving/i);
});
