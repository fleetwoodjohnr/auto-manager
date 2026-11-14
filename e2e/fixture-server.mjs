// A local stand-in for the NHTSA and EPA services, serving the responses recorded in
// crates/core/tests/fixtures, so end-to-end runs are deterministic and offline.
import http from "node:http";
import { readFileSync, existsSync } from "node:fs";
import path from "node:path";
import { root, tinyPdf } from "./harness.mjs";

const fx = (p) => readFileSync(path.join(root, "crates/core/tests/fixtures", p), "utf8");

export function startFixtures(port = 8799) {
  const hits = [];
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, "http://x");
    hits.push(url.pathname + url.search);
    const send = (body, status = 200, type = "application/json") => {
      res.writeHead(status, { "content-type": type });
      res.end(body);
    };
    let m;
    if ((m = url.pathname.match(/\/vpic\/DecodeVinValuesExtended\/([A-Z0-9*]+)/i))) {
      const f = `vpic/decode_${m[1].toUpperCase()}.json`;
      return existsSync(path.join(root, "crates/core/tests/fixtures", f)) ? send(fx(f)) : send(fx("vpic/decode_unknown.json"));
    }
    if (url.pathname.includes("/vpic/GetMakesForVehicleType/")) return send(JSON.stringify({ Results: [{ MakeName: "HONDA" }, { MakeName: "NISSAN" }] }));
    if (url.pathname.includes("/vpic/GetModelsForMakeYear/")) return send(fx("vpic/models_honda_2003.json"));
    if (url.pathname.includes("/vpic/GetCanadianVehicleSpecifications")) return send(fx("vpic/canada_2019_nissan_sentra.json"));
    if (url.pathname === "/nhtsa/recalls/recallsByVehicle") return send(fx(url.searchParams.get("make")?.toLowerCase() === "nissan" ? "nhtsa/recalls_2019_nissan_sentra.json" : "nhtsa/recalls_2003_honda_accord.json"));
    if (url.pathname === "/nhtsa/vehicles/byYmmt") return send(fx("nhtsa/bulletins_2019_nissan_sentra.json"));
    if (url.pathname === "/nhtsa/safetyIssues/byNhtsaId") return send(fx("nhtsa/bulletin_11031704.json").replaceAll("https://static.nhtsa.gov", `http://127.0.0.1:${port}/static`));
    if (url.pathname.startsWith("/static/odi/tsbs/")) return send(tinyPdf("Certification label and VIN plate replacement. Service information."), 200, "application/pdf");
    if (url.pathname.startsWith("/nhtsa/SafetyRatings/modelyear")) return send(fx("nhtsa/ratings_list.json"));
    if (url.pathname.startsWith("/nhtsa/SafetyRatings/VehicleId")) return send(fx("nhtsa/rating_13418.json"));
    if (url.pathname === "/epa/vehicle/menu/options") return send(fx("epa/options_2019_nissan_sentra.json"));
    if (url.pathname.startsWith("/epa/vehicle/")) return send(fx("epa/vehicle_40565.json"));
    send(JSON.stringify({ error: "not in fixtures" }), 404);
  });
  server.listen(port, "127.0.0.1");
  return {
    hits,
    env: {
      AUTO_MANAGER_ALLOW_HTTP_LOCALHOST: "1",
      AUTO_MANAGER_VPIC_BASE: `http://127.0.0.1:${port}/vpic`,
      AUTO_MANAGER_NHTSA_BASE: `http://127.0.0.1:${port}/nhtsa`,
      AUTO_MANAGER_NHTSA_STATIC_BASE: `http://127.0.0.1:${port}/static`,
      AUTO_MANAGER_FUELECONOMY_BASE: `http://127.0.0.1:${port}/epa`,
    },
    close: () => server.close(),
  };
}
