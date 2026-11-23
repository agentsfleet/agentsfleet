export type FaqItem = {
  question: string;
  answer: string;
  linkText?: string;
  href?: string;
};

export const faq: FaqItem[] = [
  {
    question: "What is agentsfleet?",
    answer:
      "agentsfleet is open-source software for running AI agents. They can investigate support tickets and show your team what they found.",
  },
  {
    question: "What is a fleet?",
    answer:
      "A fleet is an AI agent with a job, a budget, and tools you choose. It keeps a record of each run.",
  },
  {
    question: "What starts a fleet run?",
    answer:
      "A message, a schedule, or a webhook from another tool can start a fleet run.",
    linkText: "fleet run",
    href: "https://docs.agentsfleet.net/fleets/running",
  },
  {
    question: "Can fleets ask each other for help?",
    answer:
      "Yes. Mira handles the Slack thread and asks Scout to check Grafana. Each uses its own permissions.",
  },
  {
    question: "Who controls access?",
    answer:
      "You choose which tools, repositories, and credentials a fleet can use. Set approval rules for actions you want to review.",
    linkText: "approval rules",
    href: "https://docs.agentsfleet.net/fleets/approvals",
  },
  {
    question: "What can a fleet check in Grafana?",
    answer:
      "With read access, it can check metrics in Prometheus and logs in Loki. Here, Scout checks when the connection dropped and how long people waited.",
  },
  {
    question: "Can I self-host agentsfleet?",
    answer:
      "Yes. Run the MIT-licensed source code on your own infrastructure and choose your model.",
    linkText: "MIT-licensed source code",
    href: "https://github.com/agentsfleet/agentsfleet",
  },
  {
    question: "How much does agentsfleet cost?",
    answer:
      "Fleet runtime starts at $0. If you use your own model key, you pay your provider directly.",
  },
];
