import { showValue, toStored } from "./units";

describe("units", () => {
  it("converts for display only", () => {
    expect(showValue("3090", "lb", "imperial")).toBe("3,090 lb");
    expect(showValue("3090", "lb", "metric")).toBe("1,402 kg");
    expect(showValue("2.998832712", "L", "imperial")).toBe("3 L");
    expect(showValue("4.4", "qt", "metric")).toBe("4.2 L");
    expect(showValue("240–260", "hp", "metric")).toBe("240–260 hp");
    expect(showValue("Gasoline", null, "metric")).toBe("Gasoline");
  });
  it("stores what was typed in the source unit", () => {
    expect(toStored("4.2", "qt", "metric")).toBe("4.438");
    expect(toStored("4.4", "qt", "imperial")).toBe("4.4");
  });
});
