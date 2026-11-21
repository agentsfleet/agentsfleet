import {cleanup, render, screen} from "@testing-library/react";
import {afterEach, expect, it} from "vitest";
import {FleetSetupDetails} from "@/components/domain/setup/summary/FleetSetupDetails";
import {REVIEW} from "./helpers/draft-fixtures";

afterEach(cleanup);

// Value: protects=inspection remains truthful for saved and incomplete setup responses;
// fails_when=missing data is reported as reviewed or a stale proposal looks current; seam=record projection; no_claim=runtime model selection.
it("keeps unavailable definitions and instructions explicit", () => {
  render(<FleetSetupDetails />);
  expect(screen.getByText("New fleet")).toBeTruthy();
  expect(screen.getAllByText("Runtime model unavailable in this view").length).toBeGreaterThan(0);
  expect(screen.getByText("Source revision unavailable")).toBeTruthy();
  expect(screen.getByText("Add instructions before saving.")).toBeTruthy();
  expect(screen.queryByRole("list", {name: "Current readiness"})).toBeNull();
});

it("inspects the actual saved definition with source and revision provenance", () => {
  const definition = {...REVIEW.effective.definition, config: {"x-agentsfleet": {
    model: "saved-runtime", tools: ["shell"], budget: {daily_nanos: "2000000000"}}}};
  render(<FleetSetupDetails definition={definition} name="Merge watch" revision="2" sourceRevision="source-v3" />);
  expect(screen.getByText("Merge watch")).toBeTruthy();
  expect(screen.getAllByText("saved-runtime")).toHaveLength(2);
  expect(screen.getByText("Revision 2")).toBeTruthy();
  expect(screen.getByText("source-v3")).toBeTruthy();
  expect(screen.getByText("$2.00 / day")).toBeTruthy();
});

it.each([{}, {tools: [], budget: {daily_nanos: "1000000000"}}])("does not infer an unresolved model from a saved response without a model: %j", extension => {
  render(<FleetSetupDetails definition={{...REVIEW.effective.definition, config: {"x-agentsfleet": extension}}} />);
  expect(screen.getAllByText("Runtime model unavailable in this view")).toHaveLength(2);
  expect(screen.queryByText("Account default is not resolved")).toBeNull();
});

it("warns when the reviewed target differs from the saved revision", () => {
  render(<FleetSetupDetails revision="2" review={{...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}} />);
  expect(screen.getByText(/reviewed against revision 1/).textContent).toContain("refresh the setup before saving");
  expect(screen.getByText("Revision 2")).toBeTruthy();
});

it("does not warn about a proposal reviewed against the current saved revision", () => {
  render(<FleetSetupDetails revision="2" review={{...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "2"}}} />);
  expect(screen.getByText("Revision 2")).toBeTruthy();
  expect(screen.queryByText(/refresh the setup before saving/)).toBeNull();
});
