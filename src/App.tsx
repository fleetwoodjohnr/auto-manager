import { useEffect, useState, type ComponentType } from "react";
import {
  BarChart3,
  BookOpen,
  Boxes,
  Calculator,
  Car,
  LayoutDashboard,
  Moon,
  PanelLeft,
  ScanSearch,
  Settings as SettingsIcon,
  Sun,
  Wrench,
  Cog,
} from "lucide-react";
import { useApp, type Page } from "./store";
import { useSettings } from "./settings";
import { useCmd } from "./api/queries";
import type { AppInfo } from "./api/types";
import { currentTheme } from "./theme";
import { Logo, Toasts } from "./ui";
import GlobalSearch from "./features/shell/GlobalSearch";
import { scheduleUpdateChecks, useUpdates } from "./features/settings/updates";
import { UpdateNotice } from "./features/settings/UpdateNotice";
import Dashboard from "./features/dashboard/Dashboard";
import Lookup from "./features/lookup/Lookup";
import Garage from "./features/vehicles/Garage";
import VehiclePage from "./features/vehicles/VehiclePage";
import Catalog from "./features/parts/Catalog";
import PartPage from "./features/parts/PartPage";
import Inventory from "./features/inventory/Inventory";
import ItemPage from "./features/inventory/ItemPage";
import Library from "./features/manuals/Library";
import ManualViewer from "./features/manuals/Viewer";
import Service from "./features/service/Service";
import JobPage from "./features/service/JobPage";
import PrintJob from "./features/service/PrintJob";
import Pricing from "./features/pricing/Pricing";
import Reports from "./features/reports/Reports";
import Settings from "./features/settings/Settings";
import Customers from "./features/service/Customers";
import { useAutoSync } from "./features/settings/FolderSync";
import Suppliers from "./features/inventory/Suppliers";
import { t, tx } from "./i18n";

type NavItem = { page: Page; label: string; icon: ComponentType<{ size?: number; "aria-hidden"?: boolean }>; also?: Page[] };

const NAV: NavItem[] = [
  { page: "dashboard", label: t("Dashboard"), icon: LayoutDashboard },
  { page: "lookup", label: t("Vehicle Lookup"), icon: ScanSearch },
  { page: "vehicles", label: t("My Vehicles"), icon: Car, also: ["vehicle"] },
  { page: "parts", label: t("Parts Catalog"), icon: Cog, also: ["part"] },
  { page: "inventory", label: t("Inventory"), icon: Boxes, also: ["item", "suppliers"] },
  { page: "manuals", label: t("Manuals & Docs"), icon: BookOpen, also: ["manual"] },
  { page: "service", label: t("Maintenance & Service"), icon: Wrench, also: ["job", "customers", "print"] },
  { page: "pricing", label: t("Pricing & Estimates"), icon: Calculator },
  { page: "reports", label: t("Reports"), icon: BarChart3 },
  { page: "settings", label: t("Settings"), icon: SettingsIcon },
];
const NAV_KEY = "auto-manager-nav-collapsed";

const PAGES: Record<Page, ComponentType> = {
  dashboard: Dashboard,
  lookup: Lookup,
  vehicles: Garage,
  vehicle: VehiclePage,
  parts: Catalog,
  part: PartPage,
  inventory: Inventory,
  item: ItemPage,
  manuals: Library,
  manual: ManualViewer,
  service: Service,
  job: JobPage,
  print: PrintJob,
  pricing: Pricing,
  reports: Reports,
  settings: Settings,
  customers: Customers,
  suppliers: Suppliers,
};

export default function App() {
  const { route, go } = useApp();
  const { settings, save } = useSettings();
  const info = useCmd<AppInfo>("app_info");
  const autoCheck = !!info.data && !info.data.debug && !!settings?.update_check;
  // Development builds never check: they aren't installed and would only reach GitHub.
  useEffect(() => (autoCheck ? scheduleUpdateChecks(() => void useUpdates.getState().checkNow()) : undefined), [autoCheck]);
  useAutoSync();
  const [collapsed, setCollapsed] = useState(() => {
    try {
      return localStorage.getItem(NAV_KEY) === "1";
    } catch {
      return false;
    }
  });
  const [landed, setLanded] = useState(false);
  useEffect(() => {
    if (settings && !landed) {
      setLanded(true);
      if (settings.landing_page !== "dashboard") go(settings.landing_page as Page);
    }
  }, [settings, landed, go]);

  const place = NAV.find((n) => n.page === route.page || n.also?.includes(route.page)) ?? NAV[0];
  useEffect(() => {
    document.title = `${place.label} — Auto Manager`;
  }, [place.label]);
  // Alt+1 … Alt+0 open the pages in the order of the navigation.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.altKey || e.ctrlKey || e.metaKey) return;
      const i = e.key === "0" ? 9 : Number(e.key) - 1;
      if (NAV[i]) {
        e.preventDefault();
        go(NAV[i].page);
      }
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, [go]);
  const toggleNav = () => {
    setCollapsed(!collapsed);
    try {
      localStorage.setItem(NAV_KEY, collapsed ? "0" : "1");
    } catch {
      /* only a convenience */
    }
  };
  const dark = settings ? currentTheme() === "dark" : false;
  const Current = PAGES[route.page] ?? Dashboard;
  const item = (n: NavItem, i: number) => {
    const Icon = n.icon;
    const key = `Alt+${i === 9 ? 0 : i + 1}`;
    return (
      <button key={n.page} aria-current={place.page === n.page ? "page" : undefined} onClick={() => go(n.page)} title={n.label} data-key={key} aria-keyshortcuts={key}>
        <Icon size={18} aria-hidden />
        <span className="text">{n.label}</span>
      </button>
    );
  };

  return (
    <div className="shell">
      <a className="skip" href="#main">{t("Skip to content")}</a>
      <header className="titlebar">
        <span className="brand">
          <Logo size={22} />{" "}{t("Auto Manager")}</span>
        <span className="where">{place.label}</span>
        <GlobalSearch />
        <div className="tools">
          <button className="icon-btn" aria-label={dark ? t("Use the light theme") : t("Use the dark theme")} title={dark ? t("Light theme") : t("Dark theme")} onClick={() => save({ theme: dark ? "light" : "dark" })}>
            {dark ? <Sun size={16} aria-hidden /> : <Moon size={16} aria-hidden />}
          </button>
          <button className="icon-btn" aria-label={t("Open settings")} title={t("Settings")} onClick={() => go("settings")}>
            <SettingsIcon size={16} aria-hidden />
          </button>
        </div>
      </header>
      <div className="body">
        <nav className={`nav${collapsed ? " collapsed" : ""}`} aria-label={t("Main")}>
          {NAV.slice(0, -1).map(item)}
          <div className="nav-foot">
            {item(NAV[NAV.length - 1], NAV.length - 1)}
            <button className="collapse" aria-expanded={!collapsed} onClick={toggleNav} title={collapsed ? t("Show labels") : undefined}>
              <PanelLeft size={18} aria-hidden />
              <span className="text">{collapsed ? t("Show labels") : t("Hide labels")}</span>
            </button>
            {info.data && <span className="version">{tx("Version {0}", info.data.version)}</span>}
          </div>
        </nav>
        <main className="main" id="main" tabIndex={-1}>
          <Current key={`${route.page}:${route.id ?? ""}`} />
        </main>
      </div>
      <UpdateNotice onReview={() => go({ page: "settings", tab: "updates" })} />
      <Toasts />
    </div>
  );
}
