// Unit conversion for displaying specifications. Values are stored in the unit their source
// gave (see SpecField.unit); conversion happens only on screen.

export type UnitSystem = "imperial" | "metric";

const TO_METRIC: Record<string, { unit: string; factor: number; dp: number }> = {
  in: { unit: "mm", factor: 25.4, dp: 0 },
  lb: { unit: "kg", factor: 0.45359237, dp: 0 },
  gal: { unit: "L", factor: 3.785411784, dp: 1 },
  qt: { unit: "L", factor: 0.946352946, dp: 1 },
  psi: { unit: "kPa", factor: 6.894757, dp: 0 },
  hp: { unit: "kW", factor: 0.745699872, dp: 0 },
  lb_ft: { unit: "N·m", factor: 1.35581795, dp: 0 },
  mi: { unit: "km", factor: 1.609344, dp: 0 },
};

const LABEL: Record<string, string> = { in: "in", lb: "lb", gal: "gal", qt: "qt", psi: "psi", hp: "hp", lb_ft: "lb·ft", L: "L", mi: "mi", km: "km" };

/** "3,090 lb" or "1,402 kg". Non-numeric values (and ranges such as "240–260") are shown as given. */
export function showValue(value: string, unit: string | null | undefined, system: UnitSystem): string {
  if (!unit) return value;
  const n = Number(value.replace(/,/g, ""));
  if (!isFinite(n) || value.trim() === "") return `${value} ${LABEL[unit] ?? unit}`;
  if (unit === "L") return `${trim(n, 1)} L`;
  const m = TO_METRIC[unit];
  if (system === "metric" && m) return `${trim(n * m.factor, m.dp)} ${m.unit}`;
  return `${trim(n, 2)} ${LABEL[unit] ?? unit}`;
}

function trim(n: number, dp: number): string {
  return n.toLocaleString(undefined, { maximumFractionDigits: dp, minimumFractionDigits: 0 });
}

/** The unit a person types in for a field, in their chosen system. */
export function inputUnit(unit: string | null | undefined, system: UnitSystem): string | null {
  if (!unit) return null;
  if (system === "metric" && TO_METRIC[unit]) return TO_METRIC[unit].unit;
  return LABEL[unit] ?? unit;
}

/** Convert a value typed in the person's system back to the stored unit. */
export function toStored(value: string, unit: string | null | undefined, system: UnitSystem): string {
  if (!unit || system === "imperial" || !TO_METRIC[unit]) return value.trim();
  const n = Number(value.replace(/,/g, ""));
  if (!isFinite(n) || value.trim() === "") return value.trim();
  return String(Math.round((n / TO_METRIC[unit].factor) * 1000) / 1000);
}
