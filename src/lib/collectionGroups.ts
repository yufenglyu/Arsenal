import type {Collection, CollectionDetail} from "@/types";
export const UNFILED_COLLECTION = "__unfiled__";
export const COLLECTION_DRAG = "application/skillshub-collection";
export const SKILLS_DRAG = "application/skillshub-skills";
export const COLLECTION_SOURCE_DRAG = "application/skillshub-source-collection";
export function collectionScope(items: Collection[], id: string, descendants = true): Set<string> {
  const result = new Set([id]);
  if (descendants) {
    let size = -1;
    while (size !== result.size) {size = result.size;for (const item of items) if (item.parent_id && result.has(item.parent_id)) result.add(item.id);}
  }
  return result;
}
export function collectionMembers(details: CollectionDetail[], scope: Set<string>): Set<string> {
  return new Set(details.filter(item=>scope.has(item.id)).flatMap(item=>item.skills.map(skill=>skill.id)));
}
