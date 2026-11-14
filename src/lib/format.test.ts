import { ago, humanKey, money, num, roundDec, setFormatPrefs, date } from "./format";

describe("format", () => {
  it("rounds decimal strings half away from zero without floats", () => {
    expect(roundDec("2.675", 2)).toBe("2.68");
    expect(roundDec("-0.125", 2)).toBe("-0.13");
    expect(roundDec("9.995", 2)).toBe("10.00");
  });
  it("formats money and numbers", () => {
    setFormatPrefs({ currency: "USD", date_format: "iso" });
    expect(money("1234.5")).toBe("$1,234.50");
    expect(money("-12")).toBe("−$12.00");
    expect(money(null)).toBe("—");
    expect(num("2.998832712", 1)).toBe("3");
    expect(num("1.80", 2)).toBe("1.8");
  });
  it("formats dates in the chosen style", () => {
    setFormatPrefs({ currency: "USD", date_format: "us" });
    expect(date("2026-11-04")).toBe("11/04/2026");
    setFormatPrefs({ currency: "USD", date_format: "eu" });
    expect(date("2026-11-04")).toBe("04/11/2026");
    const now = Date.parse("2026-11-14T12:00:00Z");
    expect(ago("2026-11-14T11:55:00Z", now)).toBe("5 min ago");
    expect(ago("2026-11-13T12:00:00Z", now)).toBe("1 day ago");
  });
  it("turns keys into labels", () => {
    expect(humanKey("brake_package")).toBe("Brake package");
    expect(humanKey("vpic.AirBagLocFront")).toBe("Air bag loc front");
  });
});
