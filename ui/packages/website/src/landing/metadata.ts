import { faq } from "./faq";

export const structuredData = {
  "@context": "https://schema.org",
  "@graph": [
    {
      "@type": "WebSite",
      "@id": "https://www.agentsfleet.net/#website",
      name: "agentsfleet",
      url: "https://www.agentsfleet.net/",
    },
    {
      "@type": "SoftwareApplication",
      "@id": "https://www.agentsfleet.net/#software",
      name: "agentsfleet",
      url: "https://www.agentsfleet.net/",
      applicationCategory: "DeveloperApplication",
      description:
        "A fleet is an AI agent for engineering work. Start one with a message, then ask it to work on tasks, schedules, or events.",
      sameAs: "https://github.com/agentsfleet/agentsfleet",
      license: "https://opensource.org/license/mit",
    },
    {
      "@type": "FAQPage",
      "@id": "https://www.agentsfleet.net/#faq",
      url: "https://www.agentsfleet.net/",
      isPartOf: {
        "@id": "https://www.agentsfleet.net/#website",
      },
      about: {
        "@id": "https://www.agentsfleet.net/#software",
      },
      mainEntity: faq.map(({ question, answer }) => ({
        "@type": "Question",
        name: question,
        acceptedAnswer: { "@type": "Answer", text: answer },
      })),
    },
  ],
};
