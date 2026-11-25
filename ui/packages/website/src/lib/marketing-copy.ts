export const PRODUCT_NAME = "agentsfleet";
export const LOOP_ANCHOR_ID = "how-it-works";

export const FIRST_MESSAGE_FLOW = [
  {
    title: "Send a message",
    description: "Your first message creates the fleet. Keep talking to it in the same chat.",
  },
  {
    title: "Approve access in chat",
    description: "When the fleet needs a secret or a connection, it asks. You can approve its request once or deny it.",
  },
  {
    title: "Give it ongoing work",
    description: "Ask the fleet to add a trigger or schedule. For longer work, give it a task and tell it what done looks like.",
  },
] as const;

export const PRICING_COPY = {
  runtime: "Fleet runtime starts at $0.",
  note: "If you bring your own model key, your provider bills model usage directly.",
} as const;
