import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Accordion, AccordionItem, AccordionTrigger, AccordionContent } from "./Accordion";

function Sample({ defaultValue }: { defaultValue?: string }) {
  return (
    <Accordion type="single" collapsible defaultValue={defaultValue}>
      <AccordionItem value="q1">
        <AccordionTrigger>Question one?</AccordionTrigger>
        <AccordionContent>Answer one.</AccordionContent>
      </AccordionItem>
      <AccordionItem value="q2">
        <AccordionTrigger>Question two?</AccordionTrigger>
        <AccordionContent>Answer two.</AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}

describe("Accordion", () => {
  it("renders trigger buttons with the question text", () => {
    render(<Sample />);
    expect(screen.getByRole("button", { name: /Question one\?/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Question two\?/ })).toBeTruthy();
  });

  it("triggers carry aria-expanded reflecting open state", () => {
    render(<Sample defaultValue="q1" />);
    const t1 = screen.getByRole("button", { name: /Question one\?/ });
    const t2 = screen.getByRole("button", { name: /Question two\?/ });
    expect(t1.getAttribute("aria-expanded")).toBe("true");
    expect(t2.getAttribute("aria-expanded")).toBe("false");
  });

  it("AccordionItem applies bottom border utility", () => {
    render(<Sample />);
    const item = screen.getByRole("button", { name: /Question one\?/ }).closest("div[class*='border-b']");
    expect(item).toBeTruthy();
  });

  it("renders content for the open item via defaultValue", () => {
    render(<Sample defaultValue="q1" />);
    expect(screen.getByText("Answer one.")).toBeTruthy();
  });
});
