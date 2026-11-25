export const faq = [
  {
    question: "What is a fleet?",
    answer: "A fleet is an AI agent that works from a chat. Each fleet has its own conversation, model, and memory.",
  },
  {
    question: "Who controls access?",
    answer: "The fleet asks when it needs a secret or a connection. You can approve the request once in chat or deny it.",
  },
  {
    question: "What wakes a fleet?",
    answer: "A message wakes a fleet. You can also ask it to add a schedule or respond to GitHub, Slack, and webhook events.",
  },
  {
    question: "Can I choose my model?",
    answer: "Yes. Use a hosted model or bring your own model key. You can change the model for a fleet later.",
  },
] as const;
