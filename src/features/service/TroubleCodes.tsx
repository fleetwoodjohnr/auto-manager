import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Search } from "lucide-react";
import { call } from "../../api/ipc";
import type { Dtc } from "../../api/types";
import { Badge, ErrorNote, ExtLink, Spinner } from "../../ui";

const LIKELY: Record<string, "bad" | "warn" | "neutral"> = { high: "bad", medium: "warn", low: "neutral" };

/** Generic OBD-II code reference (OBDex, CC0). */
export function TroubleCodes({ initial }: { initial?: string }) {
  const [text, setText] = useState(initial ?? "");
  const [q, setQ] = useState(initial ?? "");
  const r = useQuery<Dtc[]>({ queryKey: ["dtc_search", q], queryFn: () => call<Dtc[]>("dtc_search", { query: q }), enabled: !!q, staleTime: Infinity });
  return (
    <div className="stack">
      <form
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          setQ(text.trim());
        }}
      >
        <input type="search" value={text} onChange={(e) => setText(e.target.value)} placeholder="A code such as P0420, or words such as oxygen sensor" style={{ flex: 1 }} aria-label="Trouble code" />
        <button className="btn primary" disabled={!text.trim()}>
          <Search size={16} aria-hidden /> Look up
        </button>
      </form>
      <p className="small muted">
        Generic (SAE) code meanings from <ExtLink href="https://github.com/foerbsnavi/OBDex">OBDex</ExtLink> (public domain data). Causes are possibilities to check, not a diagnosis. Manufacturer-specific codes (second character 1) can mean different things by make and need the manufacturer's information.
      </p>
      {r.isFetching && <Spinner label="Looking up" />}
      {r.isError && <ErrorNote error={r.error} />}
      {r.data?.length === 0 && <p className="muted">No code matches. Check the code has a letter (P, B, C or U) and four characters.</p>}
      {r.data?.map((d) => (
        <section key={d.code} className="card stack" style={{ gap: 6 }}>
          <div className="spread">
            <h2 style={{ margin: 0 }}>
              <span className="mono">{d.code}</span> {d.title}
            </h2>
            <div className="row">
              <Badge>{d.system}</Badge>
              <Badge tone={d.scope.startsWith("Generic") ? "info" : "warn"}>{d.scope}</Badge>
              {d.detail.mil && <Badge tone="warn">Lights the check-engine lamp</Badge>}
            </div>
          </div>
          {d.description && <p style={{ margin: 0 }}>{d.description}</p>}
          {d.detail.causes.length > 0 && (
            <div>
              <p className="label">Possible causes (not a diagnosis)</p>
              <ul className="plain">
                {d.detail.causes.map(([l, c], i) => (
                  <li key={i}>
                    {c} {l && <Badge tone={LIKELY[l] ?? "neutral"}>{l} likelihood</Badge>}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {d.detail.symptoms.length > 0 && (
            <div>
              <p className="label">Symptoms you may notice</p>
              <ul className="plain">
                {d.detail.symptoms.map((s, i) => (
                  <li key={i}>{s}</li>
                ))}
              </ul>
            </div>
          )}
          <div className="row small">
            {d.detail.related.length > 0 && (
              <>
                <span className="muted">Related:</span>
                {d.detail.related.map((c) => (
                  <button key={c} className="btn link mono" style={{ padding: 0 }} onClick={() => (setText(c), setQ(c))}>
                    {c}
                  </button>
                ))}
              </>
            )}
          </div>
          {d.detail.sources.length > 0 && (
            <details className="small">
              <summary>Diagnostic references ({d.detail.sources.length})</summary>
              <ul className="plain">
                {d.detail.sources.map((s) => (
                  <li key={s}>
                    <ExtLink href={s} className="btn link small">
                      {s.replace(/^https?:\/\//, "").slice(0, 80)}
                    </ExtLink>
                  </li>
                ))}
              </ul>
            </details>
          )}
        </section>
      ))}
    </div>
  );
}
