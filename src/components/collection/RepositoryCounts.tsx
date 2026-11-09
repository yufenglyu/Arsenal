import {useTranslation} from "react-i18next";

/** A consistent pair of counts for the library, groups and recycle bin. */
export function RepositoryCounts({repositories, skills}: {repositories: number; skills: number}) {
  const {t} = useTranslation();
  const summary = t("groups.countSummary", {repositories, skills});
  return (
    <span data-repository-count role="img" aria-label={summary} title={summary}
      className="shrink-0 whitespace-nowrap text-xs font-normal tabular-nums">
      <span aria-hidden="true">{t("groups.countCompact", {repositories, skills})}</span>
    </span>
  );
}
