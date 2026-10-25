const URL_TYPES = ["text/uri-list", "text/x-moz-url", "text/plain"];

export function acceptsGitHubUrlDrag(data: Pick<DataTransfer, "types">): boolean {
  const types = Array.from(data.types);
  return !types.includes("application/arsenal-skills")
    && (types.includes("Files") || URL_TYPES.some(type => types.includes(type)));
}

export function droppedGitHubUrls(data: Pick<DataTransfer, "getData">): string[] {
  const read = (type: string) => { try { return data.getData(type); } catch { return ""; } };
  const uriList = read("text/uri-list");
  const mozUrl = uriList ? "" : read("text/x-moz-url");
  const lines = (uriList || mozUrl || read("text/plain")).split(/\r?\n/);
  const urls = new Map<string, string>();
  lines.forEach((line, index) => {
    if (mozUrl && index % 2 !== 0) return; // Firefox pairs a URL with its page title.
    const value = line.trim();
    if (!value || value.startsWith("#")) return;
    try {
      const url = new URL(/^github\.com\//i.test(value) ? `https://${value}` : value);
      if (!["http:", "https:"].includes(url.protocol) || url.hostname.toLowerCase() !== "github.com"
        || url.port || url.username || url.password) return;
      const parts = url.pathname.split("/").filter(Boolean);
      if (parts.length < 2 || !/^[a-z0-9][a-z0-9-]*$/i.test(parts[0]) || !/^[a-z0-9_.-]+$/i.test(parts[1])) return;
      // Remove tracking/query data before passing the public repository address on.
      url.protocol = "https:";
      url.search = "";
      url.hash = "";
      url.pathname = url.pathname.replace(/\/+$/, "");
      const normalized = url.href;
      const key = normalized.replace(/^(https:\/\/github.com\/[^/]+\/[^/]+)/i, match => match.toLowerCase());
      urls.set(key, normalized);
    } catch { /* A dragged title or ordinary text is not a URL. */ }
  });
  return [...urls.values()];
}

/** Windows browsers may offer an InternetShortcut instead of URL text. */
export async function droppedShortcutGitHubUrls(files: readonly File[]): Promise<string[]> {
  const urls = new Set<string>();
  for (const file of files) {
    if (!file.name.toLowerCase().endsWith(".url") || file.size > 64 * 1024) continue;
    const bytes = new Uint8Array(await file.arrayBuffer());
    const encoding = bytes[0] === 0xff && bytes[1] === 0xfe ? "utf-16le"
      : bytes[0] === 0xfe && bytes[1] === 0xff ? "utf-16be" : "utf-8";
    const contents = new TextDecoder(encoding).decode(bytes);
    let shortcut = false;
    for (const line of contents.split(/\r?\n/)) {
      const section = /^\s*\[([^\]]+)\]\s*$/.exec(line);
      if (section) { shortcut = section[1].toLowerCase() === "internetshortcut"; continue; }
      const match = shortcut && /^\s*URL\s*=\s*(.*?)\s*$/i.exec(line);
      if (match) for (const url of droppedGitHubUrls({getData:type => type === "text/plain" ? match[1] : ""})) urls.add(url);
    }
  }
  return [...urls];
}
