import {useTranslation} from "react-i18next";
import {Input} from "@/components/ui/input";
import {cn} from "@/lib/utils";

export type DetailFilter = "all" | "attention" | "failed" | "finished";

export function TaskDetailFilters({value, onChange, counts, search, onSearch}: {
  value: DetailFilter;
  onChange: (value: DetailFilter) => void;
  counts: Record<DetailFilter, number>;
  search: string;
  onSearch: (value: string) => void;
}) {
  const {t} = useTranslation();
  return <div className="flex flex-wrap items-center gap-1" role="group" aria-label={t("workflow.taskPanel.detailFilter")}>
    {(["all", "attention", "failed", "finished"] as const).map(filter => <button
      key={filter} type="button" aria-pressed={value === filter}
      className={cn("inline-flex items-center gap-2 rounded-md px-3 py-1 text-sm hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring", value === filter && "bg-accent font-semibold text-primary")}
      onClick={() => onChange(filter)}>
      {t(`workflow.taskPanel.detailStatus.${filter}`)}<span className="text-xs font-normal text-muted-foreground">{counts[filter]}</span>
    </button>)}
    <Input type="search" className="ml-auto w-full sm:w-52" aria-label={t("workflow.taskPanel.searchDetails")} placeholder={t("workflow.taskPanel.searchDetails")} value={search} onChange={event => onSearch(event.target.value)}/>
  </div>;
}
