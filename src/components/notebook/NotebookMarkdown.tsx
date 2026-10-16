import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { mathRemarkPlugins, mathRehypePlugins } from "@/lib/markdownMath";
import { useNotebookStore } from "@/stores/notebookStore";

export function NotebookMarkdown({ content }: { content: string }) {
  return <div className="notebook-markdown"><ReactMarkdown remarkPlugins={[remarkGfm, ...mathRemarkPlugins]} rehypePlugins={mathRehypePlugins}
    components={{ a: ({ href, children }) => <a href={href} onClick={e => { if (href && /^https?:\/\//i.test(href)) { e.preventDefault(); void useNotebookStore.getState().visit(href); } }}>{children}</a> }}>{content}</ReactMarkdown></div>;
}
