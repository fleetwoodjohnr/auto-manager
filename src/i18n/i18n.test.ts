// Every interface text (and fixed message from the core) has a Spanish and a French translation
// that keeps its {0}… slots. A plural ending slot ({1} = "s") may be dropped where the language
// doesn't need it, but {0} never is and no unknown slot is added.
import { coreKeys, uiKeys } from "../../scripts/i18n-keys.mjs";
import es from "./es.json";
import fr from "./fr.json";

const slots = (s: string) => new Set(s.match(/\{\d+\}/g) ?? []);
const keys = [...new Set([...uiKeys(), ...coreKeys()])] as string[];

describe.each([
  ["Spanish", es as Record<string, string>],
  ["French", fr as Record<string, string>],
])("%s", (_, cat) => {
  it("translates every text", () => {
    expect(keys.filter((k) => !(k in cat))).toEqual([]);
  });
  it("keeps the slots", () => {
    const bad = keys.filter((k) => {
      const want = slots(k);
      const got = slots(cat[k] ?? k);
      return [...got].some((s) => !want.has(s)) || (want.has("{0}") && !got.has("{0}"));
    });
    expect(bad).toEqual([]);
  });
});
