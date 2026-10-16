import { fireEvent, render, screen } from "@testing-library/react";
import { Link, MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import Home from "./Home";
import { DEFAULT_JOB, EXAMPLE_JOBS } from "../lib/example-jobs";

const PRIVATE_REQUEST = "Investigate something else";
const INSTALL_COMMAND = "curl -fsSL https://agentsfleet.dev | bash";
const INCIDENT_LINK = /Investigate an incident.*Explore example run/;
const BACK_LINK = "Back to home";
const CONTEXTUAL_EXAMPLES = [
  { job: EXAMPLE_JOBS[1], link: "Explore a test regression example" },
  { job: EXAMPLE_JOBS[2], link: "Explore a recurring work example" },
] as const;
const renderHome = (path = "/") => render(<MemoryRouter initialEntries={[path]}><Home /></MemoryRouter>);

describe("Homepage conversation entrance", () => {
  it("starts with one composer and useful examples without setup chrome", () => {
    renderHome();
    const composer = screen.getByRole("textbox");
    expect(composer).toHaveValue("");
    expect(screen.getAllByRole("textbox")).toHaveLength(1);
    expect(screen.getByRole("link", { name: INCIDENT_LINK })).toHaveAttribute("href", "/#how-it-works");
    expect(screen.queryByRole("navigation", { name: "Conversation context" })).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("button", { name: /Tools & setup|Use my tools/ })).toBeNull();
    expect(screen.getByText(INSTALL_COMMAND)).toBeInTheDocument();
    expect(screen.getByText(/Claude Code · Codex · OpenCode/)).toBeInTheDocument();
    expect(screen.queryByText(/preview|release pending/i)).toBeNull();
  });
  it("opens the incident directly, hides the composer, and restores its private text on return", () => {
    renderHome();
    const composer = screen.getByRole("textbox");
    fireEvent.change(composer, { target: { value: PRIVATE_REQUEST } });
    fireEvent.click(screen.getByRole("link", { name: INCIDENT_LINK }));
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByTestId("how-it-works")).toHaveTextContent(DEFAULT_JOB.request);
    expect(screen.getByTestId("how-it-works")).not.toHaveTextContent(PRIVATE_REQUEST);
    expect(screen.getByText(INSTALL_COMMAND)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("link", { name: BACK_LINK }));
    expect(screen.queryByTestId("how-it-works")).toBeNull();
    expect(screen.getByRole("textbox")).toHaveValue(PRIVATE_REQUEST);
    expect(screen.getAllByRole("textbox")).toHaveLength(1);
  });
  it.each(CONTEXTUAL_EXAMPLES)("keeps the $job.label example and private draft when returning home", ({ job, link }) => {
    renderHome();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: PRIVATE_REQUEST } });
    fireEvent.click(screen.getByRole("link", { name: new RegExp(job.label) }));
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByRole("heading", { name: job.title })).toBeInTheDocument();
    expect(screen.getByRole("figure")).toHaveTextContent(job.report);
    expect(screen.getByRole("figure")).not.toHaveTextContent(DEFAULT_JOB.report);
    fireEvent.click(screen.getByRole("link", { name: BACK_LINK }));
    expect(screen.getByRole("textbox")).toHaveValue(PRIVATE_REQUEST);
    expect(screen.getByRole("link", { name: link })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("link", { name: link }));
    expect(screen.getByRole("heading", { name: job.title })).toBeInTheDocument();
  });
  it("opens the incident after another suggestion without overwriting the draft", () => {
    renderHome();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: PRIVATE_REQUEST } });
    fireEvent.click(screen.getByRole("link", { name: new RegExp(EXAMPLE_JOBS[1].label) }));
    fireEvent.click(screen.getByRole("link", { name: BACK_LINK }));
    fireEvent.click(screen.getByRole("link", { name: INCIDENT_LINK }));
    expect(screen.getByRole("heading", { name: DEFAULT_JOB.title })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("link", { name: BACK_LINK }));
    expect(screen.getByRole("textbox")).toHaveValue(PRIVATE_REQUEST);
  });
  it("keeps the open conversation when navigation moves to another page anchor", () => {
    render(<MemoryRouter initialEntries={["/#how-it-works"]}><Link to="/#main-content">Skip to content</Link><Home /></MemoryRouter>);
    const conversation = screen.getByRole("figure");
    fireEvent.click(screen.getByRole("link", { name: "Skip to content" }));
    expect(screen.getByRole("figure")).toBe(conversation);
  });
  it("restores the private composer when the home link clears the example anchor", () => {
    render(<MemoryRouter><Link to="/">Home</Link><Home /></MemoryRouter>);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: PRIVATE_REQUEST } });
    fireEvent.click(screen.getByRole("link", { name: INCIDENT_LINK }));
    expect(screen.queryByRole("textbox")).toBeNull();
    fireEvent.click(screen.getByRole("link", { name: /^Home$/ }));
    expect(screen.queryByRole("figure")).toBeNull();
    expect(screen.getByRole("textbox")).toHaveValue(PRIVATE_REQUEST);
  });
  it("keeps written requests out of storage and clears them on reload", () => {
    const view = renderHome();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: PRIVATE_REQUEST } });
    expect(JSON.stringify(sessionStorage)).not.toContain(PRIVATE_REQUEST);
    expect(JSON.stringify(localStorage)).not.toContain(PRIVATE_REQUEST);
    view.unmount(); renderHome();
    expect(screen.getByRole("textbox")).toHaveValue("");
  });
});
