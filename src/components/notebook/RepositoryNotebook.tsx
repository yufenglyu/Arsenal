import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNotebookStore } from "@/stores/notebookStore";
import { NotebookEditor } from "./NotebookEditor";

export function RepositoryNotebook({ directory, skillId }: { directory: string; skillId: string }) {
  const { t } = useTranslation();
  const [id, setId] = useState<number>();
  const [failed, setFailed] = useState(false);
  const row = useNotebookStore(s => s.rows.find(r => r.id === id));
  useEffect(() => {
    let active = true;
    void useNotebookStore.getState().loadConfig();
    void useNotebookStore.getState().ensure(directory, skillId).then(row => { if (active) { setId(row?.id); setFailed(!row); } });
    return () => { active = false; };
  }, [directory, skillId]);
  return row ? <NotebookEditor repository={row} /> : <p className="p-4 text-sm text-muted-foreground">{t(failed ? "g.obsidian.unavailable" : "common.loading")}</p>;
}
