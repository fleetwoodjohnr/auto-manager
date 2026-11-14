import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { FitBadge, StockBadge, Tabs, fitLabel } from "./index";

function TabsHarness() {
  const [v, setV] = useState("a");
  return (
    <>
      <Tabs label="Test tabs" value={v} onChange={setV} tabs={[{ id: "a", label: "Alpha" }, { id: "b", label: "Beta" }, { id: "c", label: "Gamma" }]} />
      <p>Selected: {v}</p>
    </>
  );
}

describe("ui kit", () => {
  it("tabs move with the arrow keys and wrap around", () => {
    render(<TabsHarness />);
    const list = screen.getByRole("tablist", { name: "Test tabs" });
    expect(screen.getByRole("tab", { name: "Alpha" }).getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(list, { key: "ArrowRight" });
    expect(screen.getByText("Selected: b")).toBeTruthy();
    fireEvent.keyDown(list, { key: "End" });
    expect(screen.getByText("Selected: c")).toBeTruthy();
    fireEvent.keyDown(list, { key: "ArrowRight" });
    expect(screen.getByText("Selected: a")).toBeTruthy();
    // Only the selected tab is in the tab order.
    expect(screen.getAllByRole("tab").filter((t) => t.getAttribute("tabindex") === "0")).toHaveLength(1);
  });

  it("names every fit status in words", () => {
    expect(fitLabel("confirmed")).toBe("Confirmed fit");
    expect(fitLabel("not_compatible")).toBe("Not compatible");
    render(<FitBadge status="conditional" />);
    expect(screen.getByText("Conditional fit")).toBeTruthy();
  });

  it("stock badges say low and out in words, not only colour", () => {
    const { rerender } = render(<StockBadge available={0} min={2} />);
    expect(screen.getByText("Out of stock")).toBeTruthy();
    rerender(<StockBadge available={2} min={2} />);
    expect(screen.getByText("Low · 2")).toBeTruthy();
    rerender(<StockBadge available={9} min={2} />);
    expect(screen.getByText("In stock · 9")).toBeTruthy();
  });
});
