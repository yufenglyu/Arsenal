export function defaultNotebookName(fullName: string): string {
  const [owner, ...parts] = fullName.split("/");
  return Array.from(`${parts.join("/") || owner}@${owner}`, char => char.charCodeAt(0) < 32 || '<>:"/\\|?*#^'.includes(char) ? "_" : char).join("");
}

export function notebookTitle(path: string): string {
  return (path.split("/").pop() ?? path).replace(/\.md$/i, "");
}

// A leading separator denotes the configured vault root, never a system root.
export function normalizeNotebookFolder(value?: string): string {
  return (value ?? "").trim().split("\\").join("/").split("/").filter(Boolean).join("/") || "Arsenal";
}
