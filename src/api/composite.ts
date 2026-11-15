// Composite types built in the command layer (not generated).
import type { Decoded } from "./bindings/Decoded";
import type { Freshness } from "./bindings/Freshness";
import type { Vehicle } from "./bindings/Vehicle";
import type { Part } from "./bindings/Part";
import type { Fitment } from "./bindings/Fitment";
import type { Item } from "./bindings/Item";
import type { StockLevel } from "./bindings/StockLevel";
import type { Reservation } from "./bindings/Reservation";
import type { Movement } from "./bindings/Movement";
import type { PriceResult } from "./bindings/PriceResult";

export interface DecodeOutcome {
  decoded: Decoded;
  freshness: Freshness;
  vehicle: Vehicle | null;
}

export interface PartDetail {
  part: Part;
  fitments: Fitment[];
  items: Item[];
}

export interface ItemDetail {
  item: Item;
  levels: StockLevel[];
  reservations: Reservation[];
  movements: Movement[];
  price: PriceResult;
}

export interface AppInfo {
  version: string;
  schema_version: number;
  data_dir: string;
  log_dir: string;
  os: string;
  arch: string;
  bundle: string | null;
  debug: boolean;
}

export interface Source {
  id: string;
  name: string;
  kind: "user" | "provider" | "import";
  priority: number;
  url: string | null;
  license: string | null;
  enabled: boolean;
  records: number;
}
