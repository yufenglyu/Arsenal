import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeRaw from "rehype-raw";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import { FileText } from "lucide-react";
import { useRepositoryDocumentStore, type RepositoryReadme } from "@/stores/repositoryDocumentStore";

const readmeSchema = {
  ...defaultSchema,
  attributes: {...defaultSchema.attributes, "*": [...(defaultSchema.attributes?.["*"] ?? []), "align"]},
};

export function RepositoryDocument({ directory, skillId, sourceUrl, revision }: { directory: string; skillId?: string; sourceUrl?: string; revision?: string | null }) {
  const { t, i18n } = useTranslation();
  const refreshed = useRepositoryDocumentStore(state => state.revision);
  const [result, setResult] = useState<{ documents: RepositoryReadme[]; error: boolean } | null>(null);
  const [selected, setSelected] = useState("");
  useEffect(() => {
    let active = true;
    setResult(null);
    if (!directory) { setResult({ documents: [], error: false }); return; }
    void useRepositoryDocumentStore.getState().readDocuments(directory, skillId).then(
      documents => { if (active) setResult({ documents, error: false }); },
      () => { if (active) setResult({ documents: [], error: true }); },
    );
    return () => { active = false; };
  }, [directory, skillId, revision, refreshed]);
  const documents = result?.documents ?? [];
  const preferredLanguage = i18n.language.startsWith("zh") ? "zh" : "en";
  const document = documents.find(doc => doc.name === selected)
    ?? documents.find(doc => doc.language === preferredLanguage)
    ?? documents.find(doc => doc.name === "README.md")
    ?? documents.find(doc => doc.language === "default") ?? documents[0];
  const resolveUrl = (url: string, image: boolean) => {
    if (!sourceUrl || /^(?:[a-z]+:|\/\/|#)/i.test(url)) return url;
    const base = sourceUrl.replace(/[^/]*$/, document?.name ?? "README.md");
    return new URL(url, image ? base.replace("https://github.com/", "https://raw.githubusercontent.com/").replace("/blob/", "/") : base).href;
  };
  return <section className="flex min-h-0 flex-1 flex-col" aria-label={t("browser.document")}>
    <header className="flex shrink-0 items-center justify-between gap-2 border-b border-border p-4">
      <h2 className="flex min-w-0 items-center gap-2 text-sm font-semibold"><FileText className="size-[1em] shrink-0"/>{documents.length > 1 ? t("browser.readmeTitle") : document?.name ?? t("browser.readmeTitle")}</h2>
      {documents.length > 1 && <select aria-label={t("browser.chooseReadme")} value={document?.name ?? ""} onChange={event => setSelected(event.target.value)}
        className="h-8 min-w-0 max-w-[70%] rounded-md border border-input bg-background px-3 text-sm font-normal text-foreground outline-none focus-visible:border-ring">
        {documents.map(doc => <option key={doc.name} value={doc.name}>{t(`browser.readmeLanguage.${doc.language}`)} · {doc.name}</option>)}
      </select>}
    </header>
    <div className="min-h-0 flex-1 overflow-auto p-4" key={document?.name}>
    {!result ? <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      : result.error ? <p role="alert" className="text-sm text-destructive">{t("browser.readmeError")}</p>
      : !document ? <p className="text-sm text-muted-foreground">{t("browser.noReadme")}</p>
      : <div className="markdown-body"><ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeRaw, [rehypeSanitize, readmeSchema]]} components={{
          div: ({node, children, ...props}) => {
            const align = String(node?.properties.align ?? "");
            return <div {...props} style={align === "center" || align === "left" || align === "right" ? {textAlign: align} : undefined}>{children}</div>;
          },
          a: ({ href, children, node: _node, ...props }) => <a {...props} href={href ? resolveUrl(href, false) : undefined} target="_blank" rel="noreferrer">{children}</a>,
          img: ({ src, node: _node, ...props }) => <img {...props} src={src ? resolveUrl(src, true) : undefined}/>,
        }}>{document.content}</ReactMarkdown></div>}
    </div>
  </section>;
}
