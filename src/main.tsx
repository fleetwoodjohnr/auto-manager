import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { startTheme } from "./theme";
import "@fontsource-variable/inter";
import "./styles.css";

startTheme();

// Data lives in the local database, so there is nothing to refetch on focus; views refresh
// after every change instead (see api/queries.ts).
const client = new QueryClient({ defaultOptions: { queries: { refetchOnWindowFocus: false, retry: false, staleTime: 5_000 } } });

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
);
