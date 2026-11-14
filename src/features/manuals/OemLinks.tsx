import { ExternalLink } from "lucide-react";
import { useCmd } from "../../api/queries";
import type { OemLink } from "../../api/types";
import { ExtLink } from "../../ui";
import { t, tx } from "../../i18n";

/** The manufacturer's official owner-information site, when a verified link is known. */
export function OemLinks({ make }: { make: string }) {
  const q = useCmd<OemLink | null>("oem_link", { make }, !!make);
  if (!make) return null;
  return (
    <div className="notice mt-2">
      <p className="label">{t("Official manuals")}</p>
      {q.data ? (
        <p className="small" style={{ margin: 0 }}>
          {q.data.make.charAt(0) + q.data.make.slice(1).toLowerCase()}{" "}{t("publishes owner's manuals on its own site:")}{" "}
          <ExtLink href={q.data.url}>{t("open the official page")}{" "}<ExternalLink size={12} aria-hidden />
          </ExtLink>{t(". Download the manual there, then import the PDF here. (Link checked")}{" "}{q.data.checked_on}.)
        </p>
      ) : (
        <p className="small" style={{ margin: 0 }}>{tx("No verified link to {0}'s owner-manual site is on file. Look for “owners” or “manuals” on the manufacturer's website, then import the PDF. Service manuals are usually sold by the manufacturer's technical information site.", make)}</p>
      )}
    </div>
  );
}
