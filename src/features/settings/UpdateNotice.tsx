import { useUpdates } from "./updates";
import { t } from "../../i18n";

export function UpdateNotice({ onReview }: { onReview: () => void }) {
  const { update, state } = useUpdates();
  if (!update || state === "downloading" || state === "installing") return null;
  return (
    <aside className="update-notice" role="status">
      <p>{t("Auto Manager {0} is available.", update.version)}</p>
      <button className="btn primary" onClick={onReview}>{t("Review update")}</button>
    </aside>
  );
}
