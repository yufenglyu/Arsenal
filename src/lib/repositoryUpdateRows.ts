import type {RepositorySyncPreview, RepositorySyncPreviewItem} from "@/types";
export type UpdateCategory = "added" | "modified" | "deleted" | "unchanged" | "updated";
export const updateCategories: UpdateCategory[] = ["added","modified","deleted","unchanged","updated"];
export const updateItemKey = (repo:string,item:RepositorySyncPreviewItem) => `${repo.toLowerCase()}:${item.skillId}:${item.version ?? ""}`;
export function repositoryUpdateRows(repositories:RepositorySyncPreview[], pairs:Record<string,string>) {
  return repositories.flatMap(repo => {
    if (repo.error) return [];
    const candidates: RepositorySyncPreviewItem[] = [];
    return updateCategories.flatMap(category => (repo[category] ?? [])
      .filter(item=>category!=="added" || !candidates.includes(item))
      .map(item=>{
        const pairKey=updateItemKey(repo.repository,item);
        const replacement=category==="deleted" ? candidates.find(candidate=>updateItemKey(repo.repository,candidate)===pairs[pairKey]) ?? undefined : undefined;
        return {repo:repo.repository,error:repo.error,item,category,pairKey,replacement,
          candidates:category==="deleted" ? candidates : [],
          key:replacement ? `${pairKey}:replace:${updateItemKey(repo.repository,replacement)}` : pairKey};
      }));
  });
}
