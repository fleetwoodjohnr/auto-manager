# User guide

Auto Manager follows one path: **find a vehicle → understand it → find its parts and manuals → check stock → price and service it.**

Press **Ctrl K** (⌘K on a Mac) anywhere to search everything. **Alt 1–9** and **Alt 0** open the ten sections in the order of the sidebar.

## 1. Look up a vehicle

**Vehicle Lookup → By VIN.** Type or scan the 17-character VIN.

- The check digit is verified as you type. A mismatch is a *warning*, not an error: it's normal for many vehicles built for markets outside North America, so check for a typo and decode anyway.
- **Decode** asks NHTSA's free vPIC service. You'll see what NHTSA knows, and a line naming what it *doesn't* provide for this VIN (for example drivetrain, or oil capacity). Nothing is guessed.
- Use `*` for characters you can't read (a partial VIN). Partial lookups are shown but not saved.
- **Without a VIN**: for pre-1981, non-US or off-road vehicles, or when you're offline. Enter the year, make and model (makes and models are suggested when online) and anything else you know.
- **Many VINs (CSV)**: pick a spreadsheet and its VIN column. VINs are decoded 50 at a time, and identified vehicles can be saved straight to My Vehicles.

**Save vehicle** puts it in **My Vehicles** (the garage). Unsaved lookups stay under *Recent lookups*.

## 2. Understand it

Open a vehicle. Its tabs:

- **Overview**: key specifications, counts of parts in stock, documents and jobs, and any missing details that affect parts matching. Also photos and documents attached to the vehicle.
- **Full Specifications**: every known value, grouped, each with a badge saying where it came from:
  - *Decoded from VIN*: from NHTSA for this VIN;
  - *Model-level*: true for the model or configuration, not confirmed for this VIN;
  - *Entered by you*;
  - *From a manual*, with the page you cited.

  Values nobody has supplied say **Not available from free sources**. Use **+** to add them from your manual, and cite the page.

  To correct a value, use the pencil. Your value is shown, and the original stays on record; the ↶ button switches back to it. When sources disagree, the row is flagged.

  Under **Add data from other free sources** you can add:
  - dimensions and weight from vPIC's Canadian specifications;
  - engine, transmission and fuel economy from the EPA.

  Pick the body style or configuration that matches.
- **Recalls & Safety**: NHTSA recall campaigns for this year, make and model, and the NHTSA 5-Star ratings. Campaigns are listed per *model*. Check the VIN on [nhtsa.gov/recalls](https://www.nhtsa.gov/recalls), then record the answer for each campaign: outstanding, completed, or doesn't apply.
- **Maintenance**: service intervals from the owner's manual, service history, and what's due (also shown on the Dashboard).
- **Service History**: estimates, jobs and invoices for the vehicle, and its parts lists.

To edit the nickname, plate, mileage, purchase details, notes or owner, use **⋯ → Edit details**.

## 3. Find its parts and manuals

- **Parts**: every catalog part with fitment records for this vehicle.
- **Parts in Stock**: only the ones you stock, with quantity, location and price. You can **Reserve** a part, **Add to estimate**, save it to a list, or request restocking.

Parts are grouped by how sure the match is:

| Status | Meaning |
|---|---|
| **Confirmed fit** | A *verified* fitment record matches this vehicle's configuration |
| **Conditional fit** | Fits some configurations. The card says which detail decides it (for example *Brake package = Sport*). Enter it in the yellow box and the match updates |
| **Possible fit** | A record matches but nobody has verified it. Check before fitting |
| **Not compatible** | Excluded, or verified records say a different configuration |
| **Unknown** | No fitment records cover this vehicle (not listed) |

A part is never shown as fitting just because the make and model match.

**Manuals**: the documents linked to this vehicle, verified links to the manufacturer's official owner-manual site, and **Where Is It?**. Type a component (for example *starter motor*). Auto Manager looks only in this vehicle's documents and your saved component locations, and shows the pages. If nothing is recorded, it says so instead of guessing.

## Parts catalog

**Parts Catalog** lists parts by system → component.

Each part has numbers (OEM, aftermarket, barcode, interchange, supplier), details, installation notes, fitment records, stock, alternatives (supersession and interchangeable parts), and photos and diagrams. Use **Does it fit?** to check the part against any saved vehicle.

**Add a fitment record**:

- years, make and model;
- optional conditions, all of which must match (engine size, drivetrain, trim, brake package, …; separate alternatives with `|`);
- position and quantity;
- where the information came from.

Tick *verified* only when you've checked it against an authoritative source. Only verified records can give a *Confirmed fit*.

**Import fitment** accepts our spreadsheet format, PIES (product data) and ACES (fitment; needs your licensed VCdb export). See [PROVIDERS.md](PROVIDERS.md).

## 4. Check stock

**Inventory** lists every stocked item. You can sort, filter by category or location, include archived items, and right-click a row for actions.

On an item's page:

| Action | What it does |
|---|---|
| **Receive** | Stock in, with its cost. Updates the average cost |
| **Issue** | Stock out |
| **Reserve** | Hold stock for a job or vehicle. It stays on the shelf but can't be issued |
| ⋯ menu | Move between locations, Adjust quantity (reason required), Customer return, Return to supplier, Mark damaged (moves it to quarantine), Print label |

Every change is in the item's **History**, and each can be undone once.

Other tabs on the Inventory page:

- **Low stock**: at or below minimum.
- **Reorder & purchasing**: what to order (see below), and purchase orders.
- **Reservations**: everything held, and what it's held for.
- **Physical count**: enter what's on the shelves; the differences are recorded.
- **Scan**: a USB barcode scanner (or typing) finds the item.

**Import** takes a CSV or Excel sheet. You match its columns, review every row (duplicate SKUs and problems are shown), then import it all in one step.

**Suppliers** holds contacts, lead times, each supplier's part numbers and prices (compared on the item page), and price-list import.

**Reorder assistant.** It suggests items at or below their minimum, counting what's already on order, plus restocking requests. The suggested quantity covers the shortfall plus expected use over the lead time, and at least the item's reorder quantity. **Create draft orders** makes one purchase order per supplier. Then mark each order **sent**, and **Receive** what arrives (partial deliveries are fine). Overdue orders are flagged as *late*.

## 5. Price and service

**Pricing & Estimates.** Profiles hold the tax rate (added to prices or included in them), rounding (to an increment, or a price ending such as .99) and the labor rate.

Rules give the price, by priority:

- for all products, a category, a brand, or one item;
- a **markup** on cost, a **target margin**, or a **fixed price**;
- optionally only from a quantity, or with a minimum price.

A fixed price set on an item beats every rule. With no rule, an item has no price: nothing is assumed. **Try it** shows the working for any cost.

- *Markup* is on cost: 40% markup on $50 is $70.
- *Margin* is on price: a 40% margin on $50 is $83.33.

**Maintenance & Service → New estimate.** Choose the customer and vehicle, then add parts from inventory (priced by your rules, with "how priced"), labor (hours × rate) and fees. Reserve parts as you go.

The job then moves through these steps:

1. **Customer approved**
2. **Start work**
3. **Complete job.** Parts are taken from stock (reserved first, then available), exactly once, and the work is added to the vehicle's history.
4. **Create invoice.** Assigns the next invoice number.

**Print / PDF** prints the estimate or invoice. Choose *Save as PDF* or *Print to File* to get a PDF.

The **Trouble codes** tab looks up generic OBD-II codes: meaning, possible causes (not a diagnosis), symptoms and references. The **OBD-II scanner** tab reads codes, freeze frame and live values through an ELM327-compatible adapter. It only reads, and never clears codes.

## Reports

Inventory value, stock status, movement, most-used parts, slow movers, turnover, revenue/cost/profit, completed jobs, estimate conversion, supplier spending, and per-vehicle history, cost of ownership, parts replaced and what's due. Print to PDF, or export CSV or Excel.

The **Data Quality Center** lists things worth fixing:

- conflicting specifications;
- missing matching details;
- parts with only unverified fitment;
- unmapped imported conditions;
- stocked parts without fitment;
- items without category, cost, price or bin.

## Settings and your data

- **General**: theme, units (shown converted; stored as the source gave them), date format, currency, start page, dashboard sections.
- **Vehicle data**: sources and their priority, and the offline cache.
- **Inventory**: locations, valuation (average or last cost), minimum stock, label barcode type.
- **Pricing**: the default profile.
- **Data and backups**:
  - daily automatic backups (14 kept), back up now or to a file, restore (always verified first, and the current data is backed up before it's replaced);
  - **Move to another computer**: an encrypted bundle protected by your passphrase;
  - export inventory as CSV or Excel, or everything as JSON;
  - check the database.
- **Updates**: current version, Check for Updates, release notes, download, restart and install, automatic checks and downloads, stable or beta channel. Updates never install without you.
