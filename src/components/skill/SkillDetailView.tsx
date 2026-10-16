import { Popover } from "@base-ui/react/popover";
import { ActionIcon } from "@/components/ui/action-icon";
import { useTaskQueueStore, isTaskActive } from "@/stores/taskQueueStore";
import { open } from "@tauri-apps/plugin-shell";
import { OpenableDirectoryPath } from "@/components/common/OpenableDirectoryPath";
import { skillSourceLinks } from "@/lib/skillSourceLinks";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type Ref, type ReactNode } from "react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  Maximize2,
  X,
  Tag,
  Plus,
  FileText,
  Bot,
  Loader2,
  ChevronDown,
  ChevronRight,
  FolderOpen,
  Lock,
  Info,
  NotebookPen,
  Layers,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { SkillFrontmatterCard } from "@/components/skill/SkillFrontmatterCard";
import { parseFrontmatter } from "@/lib/frontmatter";
import { useSkillDetailStore } from "@/stores/skillDetailStore";
import { CollectionPickerDialog } from "@/components/collection/CollectionPickerDialog";
import {
  PlatformSourceKind,
  SkillDetailRequest,
  SkillDirectoryNode,
} from "@/types";
import { cn } from "@/lib/utils";
import { invoke, isTauriRuntime } from "@/lib/tauri";
import { formatPathForDisplay } from "@/lib/path";

// In-session drafts survive selecting another tree row; explicit Save still controls persistence.
const inspectorDrafts = new Map<string, { notes?: string; tags?: string }>();

// ─── Section Label ─────────────────────────────────────────────────────────────

function SectionLabel({ children, icon }: { children: React.ReactNode; icon?: ReactNode }) {
  return (
    <div className={cn("mb-3 flex items-center gap-2 font-semibold text-muted-foreground", "text-sm")}>
      {icon}{children}
    </div>
  );
}

function SectionPanel({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("space-y-3 rounded-lg border border-border/70 bg-background p-3", className)}>
      {children}
    </div>
  );
}

// ─── MetadataRow (compact) ───────────────────────────────────────────────────

export function MetadataRow({ label, value, href, directory }: { label: string; value: string; href?: string; directory?: boolean | string }) {
  const { t } = useTranslation();
  async function openLink(event: React.MouseEvent<HTMLAnchorElement>) {
    event.preventDefault();
    // The shell plugin also opens target=_blank anchors from a body click listener.
    event.stopPropagation();
    if (!href) return;
    try {
      if (isTauriRuntime()) await open(href);
      else window.open(href, "_blank", "noopener,noreferrer");
    } catch { toast.error(t("detail.openSourceFailed")); }
  }
  return (
    <div className="space-y-1">
      <div className="text-sm font-normal text-muted-foreground/80">{label}</div>
      <div className="break-all font-mono text-sm leading-6 text-foreground">
        {directory ? <OpenableDirectoryPath path={typeof directory === "string" ? directory : value} displayPath={value} className="mt-0 block max-w-full truncate text-sm text-foreground" />
          : href ? <a href={href} target="_blank" rel="noopener noreferrer" onClick={openLink} className="hover:text-primary hover:underline">{value}</a>
          : value}
      </div>
    </div>
  );
}

function SourceOriginBadge({ originKind }: { originKind: PlatformSourceKind }) {
  const { t } = useTranslation();
  const label =
    originKind === "shared-central"
      ? t("platform.originSharedCentral")
      : t("platform.originCompatibility");
  const hint =
    originKind === "shared-central"
      ? t("platform.originSharedCentralHint")
      : t("platform.originCompatibilityHint");

  return (
    <span
      title={hint}
      className={cn(
        "inline-flex items-center rounded-full bg-violet-500/10 px-2 py-0.5 text-xs font-medium text-violet-700 ring-1 ring-violet-500/20 dark:text-violet-300"
      )}
    >
      {label}
    </span>
  );
}

function ReadOnlySourceBadge() {
  const { t, i18n } = useTranslation();

  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground ring-1 ring-border/70">
      <Lock className="size-4 shrink-0" />
      {t("detail.readOnlySource", {
        defaultValue: i18n.language.startsWith("zh") ? "只读来源" : "Read-only source",
      })}
    </span>
  );
}

const detailTypographyClassName = cn(
  "text-[13px] leading-6 text-foreground/90",
  "[&_p]:text-[13px] [&_p]:leading-6",
  "[&_li]:text-[13px] [&_li]:leading-6",
  "[&_blockquote]:text-[13px] [&_blockquote]:leading-6",
  "[&_h1]:text-lg [&_h1]:leading-7 [&_h1]:font-semibold",
  "[&_h2]:text-base [&_h2]:leading-6 [&_h2]:font-semibold",
  "[&_h3]:text-sm [&_h3]:leading-6 [&_h3]:font-semibold",
  "[&_h4]:text-[13px] [&_h4]:leading-6 [&_h4]:font-semibold",
  "[&_th]:text-xs [&_th]:leading-5",
  "[&_td]:text-xs [&_td]:leading-5",
  "[&_code]:text-[12px]",
  "[&_pre]:text-[12px] [&_pre]:leading-5",
  "[&_pre_code]:text-[12px] [&_pre_code]:leading-5"
);

const DEFAULT_DETAIL_SIDEBAR_WIDTH = 512;
const MIN_DETAIL_SIDEBAR_WIDTH = 360;
const MAX_DETAIL_SIDEBAR_WIDTH = 720;
const SOURCE_TYPE_OPTIONS = ["manual", "github", "raw"] as const;

function clampDetailSidebarWidth(width: number) {
  return Math.min(MAX_DETAIL_SIDEBAR_WIDTH, Math.max(MIN_DETAIL_SIDEBAR_WIDTH, width));
}

interface SelectedSkillFile {
  path: string;
  relativePath: string;
}

function findFileNodeByPath(nodes: SkillDirectoryNode[], path: string): SkillDirectoryNode | null {
  for (const node of nodes) {
    if (node.path === path) {
      return node;
    }
    if (node.children.length > 0) {
      const match = findFileNodeByPath(node.children, path);
      if (match) {
        return match;
      }
    }
  }
  return null;
}

function FileTreeNode({
  node,
  level,
  selectedPath,
  expandedDirectories,
  onToggleDirectory,
  onSelectFile,
}: {
  node: SkillDirectoryNode;
  level: number;
  selectedPath: string | null;
  expandedDirectories: Set<string>;
  onToggleDirectory: (path: string) => void;
  onSelectFile: (file: SelectedSkillFile) => void;
}) {
  const paddingLeft = `${level * 12}px`;

  if (node.is_dir) {
    const isExpanded = expandedDirectories.has(node.path);
    return (
      <div className="space-y-1">
        <button
          type="button"
          aria-expanded={isExpanded}
          onClick={() => onToggleDirectory(node.path)}
          className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-muted-foreground hover:bg-muted/60 hover:text-foreground cursor-pointer"
          style={{ paddingLeft }}
        >
          {isExpanded ? <ChevronDown className="size-4 shrink-0" /> : <ChevronRight className="size-4 shrink-0" />}
          <FolderOpen className="size-4 shrink-0" />
          <span className="truncate">{node.name}</span>
        </button>
        {isExpanded && node.children.map((child) => (
          <FileTreeNode
            key={child.path}
            node={child}
            level={level + 1}
            selectedPath={selectedPath}
            expandedDirectories={expandedDirectories}
            onToggleDirectory={onToggleDirectory}
            onSelectFile={onSelectFile}
          />
        ))}
      </div>
    );
  }

  const isSelected = node.path === selectedPath;
  return (
    <button
      type="button"
      onClick={() => onSelectFile({ path: node.path, relativePath: node.relative_path })}
      className={cn(
        "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors cursor-pointer",
        isSelected
          ? "bg-primary/10 text-primary"
          : "text-foreground/80 hover:bg-muted/60 hover:text-foreground"
      )}
      style={{ paddingLeft }}
      title={node.relative_path}
    >
      <FileText className="size-4 shrink-0" />
      <span className="truncate">{node.name}</span>
    </button>
  );
}

// ─── SkillDetailView ──────────────────────────────────────────────────────────

/**
 * Shared presentation component for skill detail. Rendered by both the
 * full-page route wrapper (`SkillDetailPage`) and the list-entry drawer
 * (`SkillDetailDrawer`). This component owns:
 *   - ViewHeader (title/description + optional leading slot)
 *   - TwoColumnLayout (LeftPreview tab panel + RightSidebar metadata/install/collections)
 *   - CollectionPicker portal
 *
 * It does NOT render a back button, breadcrumb, or close button. Those belong
 * to the outer shell. It also does NOT call `useNavigate` / `useParams`; all
 * route/shell concerns are handled outside.
 */
export interface SkillDetailViewProps {
  /** The skill id to load from DB. Required for central skills. */
  skillId?: string;
  /** Optional platform context for source-aware detail loading. */
  agentId?: string;
  /** Optional stable row identity for duplicate platform rows. */
  rowId?: string;
  /** Affects local styling only, never behavior. */
  variant: "page" | "drawer" | "inspector";
  inspectorTab?: "overview" | "document";
  installationSummary?: ReactNode;
  /** ViewHeader leftmost slot; currently null from both shells. */
  leading?: ReactNode;
  /** Drawer-only: used so the view can request its shell to close (e.g. on Esc). */
  onRequestClose?: () => void;
  /** Optional: exposes the left-preview scroll container to the outer shell. */
  scrollContainerRef?: Ref<HTMLDivElement>;
  /** Optional id applied to the ViewHeader h1 for shell-level aria-labelledby. */
  titleId?: string;
}

export function SkillDetailView({
  skillId,
  agentId,
  rowId,
  variant,
  inspectorTab = "overview",
  installationSummary,
  leading = null,
  onRequestClose: _onRequestClose,
  scrollContainerRef,
  titleId,
}: SkillDetailViewProps) {
  const { t, i18n } = useTranslation();

  // Store data (used in skillId mode)
  const detail = useSkillDetailStore((s) => s.detail);
  const storeContent = useSkillDetailStore((s) => s.content);
  const storeIsLoading = useSkillDetailStore((s) => s.isLoading);
  const error = useSkillDetailStore((s) => s.error);
  const loadDetail = useSkillDetailStore((s) => s.loadDetail);
  const storeExplanation = useSkillDetailStore((s) => s.explanation);
  const storeIsExplanationLoading = useSkillDetailStore((s) => s.isExplanationLoading);
  const isExplanationStreaming = useSkillDetailStore((s) => s.isExplanationStreaming);
  const explanationError = useSkillDetailStore((s) => s.explanationError);
  const explanationErrorInfo = useSkillDetailStore((s) => s.explanationErrorInfo);
  const loadCachedExplanation = useSkillDetailStore((s) => s.loadCachedExplanation);
  const clearNoteGeneration = useSkillDetailStore(s=>s.clearNoteGeneration);
  const refreshExplanation = useSkillDetailStore((s) => s.refreshExplanation);
  const generateTags = useSkillDetailStore((s) => s.generateTags);
  const updateMetadata = useSkillDetailStore((s) => s.updateMetadata);
  const updateSourceMetadata = useSkillDetailStore((s) => s.updateSourceMetadata);
  const reset = useSkillDetailStore((s) => s.reset);

  const [directoryTree, setDirectoryTree] = useState<SkillDirectoryNode[]>([]);
  const [isDirectoryTreeLoading, setIsDirectoryTreeLoading] = useState(false);
  const [directoryTreeError, setDirectoryTreeError] = useState<string | null>(null);
  const [readingOpen, setReadingOpen] = useState(false);
  const [filePickerOpen, setFilePickerOpen] = useState(false);
  const previewScrollRef = useRef<HTMLDivElement | null>(null);
  const readingOrigin = useRef({top:0,left:0});
  const readingButtonRef = useRef<HTMLButtonElement>(null);
  const readingCloseRef = useRef<HTMLButtonElement>(null);
  function closeReading() {
    setReadingOpen(false);
    requestAnimationFrame(() => {
      if (previewScrollRef.current) { previewScrollRef.current.scrollTop = readingOrigin.current.top; previewScrollRef.current.scrollLeft = readingOrigin.current.left; }
      readingButtonRef.current?.focus({preventScroll:true});
    });
  }
  useEffect(() => {
    if (!readingOpen) return;
    readingCloseRef.current?.focus({preventScroll:true});
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); closeReading(); }
    };
    window.addEventListener("keydown", escape, true);
    return () => window.removeEventListener("keydown", escape, true);
  }, [readingOpen]);
  const [selectedFile, setSelectedFile] = useState<SelectedSkillFile | null>(null);
  const [selectedFileContent, setSelectedFileContent] = useState<string | null>(null);
  const [isSelectedFileLoading, setIsSelectedFileLoading] = useState(false);
  const [expandedDirectories, setExpandedDirectories] = useState<Set<string>>(new Set());
  const detailRequest = useMemo<SkillDetailRequest | null>(
    () => (skillId ? { skillId, agentId, rowId } : null),
    [skillId, agentId, rowId]
  );
  const explanationRequestKey = useMemo(() => {
    if (!skillId) {
      return null;
    }
    return detail?.row_id ?? rowId ?? skillId;
  }, [detail?.row_id, rowId, skillId]);

  // Unified accessors
  const skillContent = storeContent;
  const isLoading = storeIsLoading;
  const explanation = storeExplanation;
  const isExplanationLoading = storeIsExplanationLoading;

  // Local UI state
  const [detailSidebarWidth, setDetailSidebarWidth] = useState(DEFAULT_DETAIL_SIDEBAR_WIDTH);
  const [isCollectionPickerOpen, setIsCollectionPickerOpen] = useState(false);
  const draftKey = JSON.stringify([skillId, agentId, rowId]);
  const [notesInput, setNotesInput] = useState("");
  const [tagsInput, setTagsInput] = useState("");
  function setDraft(field: "notes" | "tags", value: string) {
    if (variant === "inspector") inspectorDrafts.set(draftKey, { ...inspectorDrafts.get(draftKey), [field]: value });
    if (field === "notes") setNotesInput(value); else setTagsInput(value);
  }
  const [isGeneratingTags, setIsGeneratingTags] = useState(false);
  const tagsRequestId = useRef(0);
  useEffect(() => {
    tagsRequestId.current += 1;
    setIsGeneratingTags(false);
    return () => { tagsRequestId.current += 1; };
  }, [skillId, agentId, rowId]);
  const [metadataAction, setMetadataAction] = useState<"notes-save" | "tags-save" | "notes-clear" | "tags-clear" | null>(null);
  const isSavingMetadata = metadataAction !== null;
  const [isGeneratingNoteIntoNotes, setIsGeneratingNoteIntoNotes] = useState(false);
  const [sourceTypeInput, setSourceTypeInput] = useState("github");
  const [sourceUrlInput, setSourceUrlInput] = useState("");
  const [sourceRepoInput, setSourceRepoInput] = useState("");
  const [sourcePathInput, setSourcePathInput] = useState("");
  const [isSavingSourceMetadata, setIsSavingSourceMetadata] = useState(false);
  const addToCollectionButtonRef = useRef<HTMLButtonElement | null>(null);
  const selectedFilePath = selectedFile?.path ?? null;
  const selectedRelativePath = selectedFile?.relativePath ?? null;
  const currentDirectoryPath = detail?.dir_path ?? null;
  const skillFilePath = detail?.file_path ?? null;
  const sidebarStyle = {
    "--skill-detail-sidebar-width": `${detailSidebarWidth}px`,
  } as CSSProperties;

  const handleSidebarResizeStart = useCallback((event: React.MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = detailSidebarWidth;

    function handleMouseMove(moveEvent: MouseEvent) {
      setDetailSidebarWidth(clampDetailSidebarWidth(startWidth + startX - moveEvent.clientX));
    }

    function handleMouseUp() {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    }

    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
  }, [detailSidebarWidth]);

  useEffect(() => {
    if (detail?.is_read_only && isCollectionPickerOpen) {
      setIsCollectionPickerOpen(false);
    }
  }, [detail?.is_read_only, isCollectionPickerOpen]);

  useEffect(() => {
    const draft = variant === "inspector" ? inspectorDrafts.get(draftKey) : undefined;
    setNotesInput(draft?.notes ?? detail?.notes ?? "");
    setTagsInput(draft?.tags ?? (detail?.tags ?? []).join(", "));
  }, [detail?.id, detail?.notes, detail?.tags, draftKey, variant]);

  useEffect(() => {
    if (isGeneratingNoteIntoNotes && explanation && !isExplanationLoading && !isExplanationStreaming && !explanationError) {
      setNotesInput(explanation);
      if (variant === "inspector") inspectorDrafts.set(draftKey, { ...inspectorDrafts.get(draftKey), notes: explanation });
    }
    if (
      isGeneratingNoteIntoNotes
      && (explanationError || (explanation && !isExplanationLoading && !isExplanationStreaming))
    ) {
      setIsGeneratingNoteIntoNotes(false);
    }
  }, [
    explanation,
    explanationError,
    isExplanationLoading,
    isExplanationStreaming,
    isGeneratingNoteIntoNotes,
    variant,
    draftKey,
  ]);

  useEffect(() => {
    const source = detail?.source ?? "";
    setSourceTypeInput(source.startsWith("github:") || detail?.source_repo ? "github" : "manual");
    setSourceUrlInput(detail?.source_url ?? "");
    setSourceRepoInput(detail?.source_repo ?? (source.startsWith("github:") ? source.slice("github:".length) : ""));
    setSourcePathInput(detail?.source_path ?? "");
  }, [
    detail?.id,
    detail?.source,
    detail?.source_path,
    detail?.source_repo,
    detail?.source_url,
  ]);

  const fetchDirectoryTree = useCallback(async (dirPath: string) => {
    if (!isTauriRuntime()) {
      setDirectoryTree([]);
      setDirectoryTreeError(null);
      setIsDirectoryTreeLoading(false);
      return;
    }

    setIsDirectoryTreeLoading(true);
    setDirectoryTreeError(null);
    try {
      const tree = await invoke<SkillDirectoryNode[]>("list_skill_directory", { dirPath });
      setDirectoryTree(tree);
    } catch (err) {
      setDirectoryTree([]);
      setDirectoryTreeError(String(err));
    } finally {
      setIsDirectoryTreeLoading(false);
    }
  }, []);

  // ── Store mode: load detail by skillId ────────────────────────────────
  useEffect(() => {
    if (detailRequest) {
      loadDetail(detailRequest);
    }
    return () => {
      reset();
    };
  }, [detailRequest, loadDetail, reset]);

  useLayoutEffect(() => {
    if (explanationRequestKey && storeContent) {
      loadCachedExplanation(explanationRequestKey, i18n.language);
    }
  }, [explanationRequestKey, storeContent, i18n.language, loadCachedExplanation]);

  useEffect(() => {
    if (!currentDirectoryPath) {
      setDirectoryTree([]);
      setDirectoryTreeError(null);
      return;
    }

    setSelectedFile(null);
    setSelectedFileContent(null);
    setExpandedDirectories(new Set());
    void fetchDirectoryTree(currentDirectoryPath);
  }, [currentDirectoryPath, fetchDirectoryTree]);

  useEffect(() => {
    if (!skillFilePath || directoryTree.length === 0) {
      return;
    }

    if (selectedFilePath && findFileNodeByPath(directoryTree, selectedFilePath)) {
      return;
    }

    const defaultNode = findFileNodeByPath(directoryTree, skillFilePath);
    setSelectedFile({
      path: skillFilePath,
      relativePath: defaultNode?.relative_path ?? "SKILL.md",
    });
  }, [directoryTree, selectedFilePath, skillFilePath]);

  useEffect(() => {
    if (!selectedFilePath || !skillFilePath || selectedFilePath === skillFilePath) {
      setSelectedFileContent(null);
      setIsSelectedFileLoading(false);
      return;
    }
    if (!isTauriRuntime()) {
      setSelectedFileContent(null);
      setIsSelectedFileLoading(false);
      return;
    }

    let cancelled = false;
    setIsSelectedFileLoading(true);
    invoke<string>("read_file_by_path", { path: selectedFilePath })
      .then((text) => {
        if (!cancelled) {
          setSelectedFileContent(text);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setSelectedFileContent(null);
        }
      })
      .finally(() => {
        if (!cancelled) {
          setIsSelectedFileLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [selectedFilePath, skillFilePath]);

  // ── Derived values ───────────────────────────────────────────────────────

  const skillCollections = detail?.collections ?? [];

  // ── Handlers ─────────────────────────────────────────────────────────────

  function handleCollectionAdded() {
    if (detailRequest) {
      loadDetail(detailRequest);
    }
  }

  function handleCollectionPickerOpenChange(open: boolean) {
    setIsCollectionPickerOpen(open);
    if (!open) {
      queueMicrotask(() => {
        addToCollectionButtonRef.current?.focus();
      });
    }
  }

  function parseTagsInput(value: string): string[] {
    const seen = new Set<string>();
    return value
      .split(/[,，\n]/)
      .map((tag) => tag.trim().replace(/^#/, ""))
      .filter(Boolean)
      .filter((tag) => {
        const key = tag.toLowerCase();
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      })
      .slice(0, 30);
  }

  async function handleSaveNotes() {
    if (!detail || detail.is_read_only || isSavingMetadata) return;
    setMetadataAction("notes-save");
    try {
      await updateMetadata(detail.id, {
        notes: notesInput.trim() ? notesInput.trim() : null,
        tags: detail.tags ?? [],
      });
      const draft = inspectorDrafts.get(draftKey);
      if (draft) delete draft.notes;
      toast.success(t("detail.notesSaved"));
    } catch (err) {
      toast.error(t("detail.metadataSaveError", { error: String(err) }));
    } finally {
      setMetadataAction(null);
    }
  }

  async function handleGenerateTags() {
    if (!skillContent || !detail || detail.is_read_only || isGeneratingTags) return;
    const requestId = ++tagsRequestId.current;
    setIsGeneratingTags(true);
    try {
      const tags = await generateTags(skillContent);
      if (requestId === tagsRequestId.current) setDraft("tags", parseTagsInput([tagsInput, ...tags].join(", ")).join(", "));
    } catch (err) {
      if (requestId === tagsRequestId.current) {
        toast.error(t("detail.tagsGenerationError", { error: String(err) }));
      }
    } finally {
      if (requestId === tagsRequestId.current) setIsGeneratingTags(false);
    }
  }

  async function handleSaveTags() {
    if (!detail || detail.is_read_only || isSavingMetadata) return;
    setMetadataAction("tags-save");
    try {
      await updateMetadata(detail.id, {
        notes: detail.notes ?? null,
        tags: parseTagsInput(tagsInput),
      });
      const draft = inspectorDrafts.get(draftKey);
      if (draft) delete draft.tags;
      toast.success(t("detail.tagsSaved"));
    } catch (err) {
      toast.error(t("detail.metadataSaveError", { error: String(err) }));
    } finally {
      setMetadataAction(null);
    }
  }

  async function handleSaveSourceMetadata() {
    if (!detail || !canEditBasicSource) return;
    setIsSavingSourceMetadata(true);
    try {
      await updateSourceMetadata(detail.id, {
        sourceType: sourceTypeInput,
        sourceUrl: sourceUrlInput.trim() || null,
        sourceAuthor: null,
        sourceRepo: sourceRepoInput.trim() || null,
        sourcePath: sourcePathInput.trim() || null,
      });
      toast.success(t("detail.sourceMetadataSaved"));
    } catch (err) {
      toast.error(t("detail.sourceMetadataSaveError", { error: String(err) }));
    } finally {
      setIsSavingSourceMetadata(false);
    }
  }

  function displaySourceValue(source?: string | null, sourceRepo?: string | null) {
    if (sourceRepo) return sourceRepo;
    if (!source) return null;
    if (source.startsWith("github:")) return source.slice("github:".length);
    return source;
  }

  function handleGenerateNote() {
    if (!explanationRequestKey || !skillContent || isExplanationLoading || isExplanationStreaming) return;
    setIsGeneratingNoteIntoNotes(true);
    void refreshExplanation(explanationRequestKey, skillContent, i18n.language);
  }

  async function handleClear(field: "notes" | "tags") {
    if (!detail || detail.is_read_only || isSavingMetadata) return;
    for (const task of useTaskQueueStore.getState().tasks) {
      if (isTaskActive(task) && task.key === `ai:${field}:${field === "notes" ? explanationRequestKey : detail.id}`) useTaskQueueStore.getState().cancel(task.id);
    }
    if (field === "notes") {setIsGeneratingNoteIntoNotes(false);clearNoteGeneration?.();}
    else { tagsRequestId.current += 1; setIsGeneratingTags(false); }
    setMetadataAction(`${field}-clear`);
    try {
      await updateMetadata(detail.id, {notes: field === "notes" ? null : detail.notes ?? null, tags: field === "tags" ? [] : detail.tags ?? []});
      setDraft(field, "");
    } catch { toast.error(t("workflow.operationFailed")); }
    finally {setMetadataAction(null);}
  }

  function handleSelectFile(file: SelectedSkillFile) {
    setSelectedFile(file);
  }

  function handleToggleDirectory(path: string) {
    setExpandedDirectories((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }

  const previewContent = selectedFilePath && skillFilePath && selectedFilePath !== skillFilePath
    ? selectedFileContent
    : skillContent;
  const selectedPreviewPath = selectedFilePath ?? skillFilePath;
  const isSelectedMarkdownFile = (selectedPreviewPath ?? "").toLowerCase().endsWith(".md");
  const { frontmatterRaw, frontmatterData, body: markdownContent } = previewContent && isSelectedMarkdownFile
    ? parseFrontmatter(previewContent)
    : { frontmatterRaw: "", frontmatterData: {}, body: previewContent ?? "" };
  const isBrowserFallback = !isTauriRuntime() && !isLoading && !detail && !error;
  const effectiveName = detail?.name ?? detailRequest?.skillId ?? "";
  const effectiveDescription = detail?.description;
  const hasData = !!detail;
  const canEditBasicSource = !!detail
    && !detail.is_read_only
    && !detail.is_central
    && !detail.source_kind
    && (detail.source === "manual" || (detail.source === "resource-library" && !detail.source_repo));
  const displayedSource = detail
    ? displaySourceValue(detail.source, detail.source_repo)
    : null;
  const sourceLinks = skillSourceLinks(detail ?? {});
  const storageMetadataRows = useMemo(() => {
    if (!detail) return [];
    const seen = new Set<string>();
    return [
      [
        t("detail.storagePath"),
        detail.dir_path,
      ],
      [
        t("detail.installationPath"),
        detail.canonical_path,
      ],
    ]
      .filter((row): row is [string, string] => Boolean(row[1]))
      .map(([label, value]) => [label, formatPathForDisplay(value)] as [string, string])
      .filter(([, value]) => {
        if (seen.has(value)) return false;
        seen.add(value);
        return true;
      });
  }, [detail, t]);

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className={cn("flex flex-col h-full", variant === "drawer" && "min-h-0")}>
      {/* ── ViewHeader: leading slot + title/description ─────────────────── */}
      <div className={cn("border-b border-border flex items-center gap-3 shrink-0", variant === "inspector" ? "gap-2 px-4 py-4" : "px-6 py-3")}>
        {leading}
        {variant === "inspector" && <FileText className="size-[1em] shrink-0 text-muted-foreground" aria-hidden="true"/>}
        <div className="min-w-0 flex-1">
          <h1 id={titleId} className={cn("font-semibold", variant === "inspector" ? "overview-item-title break-words" : "truncate text-sm")}>
            {isLoading ? (skillId ?? "") : effectiveName}
          </h1>
          {variant !== "inspector" && effectiveDescription && (
            <p className="mt-0.5 truncate text-xs text-muted-foreground">
              {effectiveDescription}
            </p>
          )}
        </div>
      </div>

      {/* ── ContentArea ──────────────────────────────────────────────────── */}
      <div className="flex-1 min-h-0 overflow-hidden">
        {/* Loading state */}
        {isLoading && (
          <div className="flex items-center justify-center h-full gap-2 text-muted-foreground">
            <Loader2 className="size-4 animate-spin" />
            <span className="text-sm">{t("detail.loading")}</span>
          </div>
        )}

        {/* Error state */}
        {!isLoading && error && (
          <div className="flex items-center justify-center h-full">
            <div className="text-center space-y-2">
              <p className="text-sm text-destructive">{error}</p>
              <Button
                variant="outline"
                size="sm"
                onClick={() => detailRequest && loadDetail(detailRequest)}
              ><ActionIcon action="retry"/>
                {t("detail.retry")}
              </Button>
            </div>
          </div>
        )}

        {!isLoading && !error && isBrowserFallback && (
          <div className="flex items-center justify-center h-full">
            <div className="text-center space-y-3 max-w-md px-6">
              <Bot className="size-8 mx-auto text-muted-foreground/60" />
              <div className="space-y-1">
                <p className="text-sm font-medium">{t("detail.browserFallbackTitle")}</p>
                <p className="text-sm text-muted-foreground">
                  {t("detail.browserFallbackDesc")}
                </p>
              </div>
            </div>
          </div>
        )}

        {/* ── TwoColumnLayout: LeftPreview + RightSidebar ────────────────── */}
        {!isLoading && !error && hasData && (
          <div
            className={cn("flex h-full flex-col", variant !== "inspector" && "md:flex-row")}
            data-testid="skill-detail-two-column-layout"
          >
            {/* ── Left: SKILL.md Preview ─────────────────────────────── */}
            <div
              ref={node => { previewScrollRef.current = node; if(typeof scrollContainerRef === "function") scrollContainerRef(node); else if(scrollContainerRef) (scrollContainerRef as {current:HTMLDivElement | null}).current = node; }}
              role={readingOpen ? "dialog" : undefined}
              aria-label={readingOpen ? t("detail.readingView") : undefined}
              className={cn("min-h-0 flex-1 min-w-0 overflow-auto", variant === "inspector" && inspectorTab !== "document" && "hidden", readingOpen && "fixed inset-y-0 right-0 z-50 w-1/2 border-l border-border bg-background shadow-xl")}
            >
              {variant === "inspector" && <div className="sticky top-0 z-10 flex h-11 items-center justify-between gap-2 border-b border-border bg-background px-4 text-sm">
                <div className="flex min-w-0 items-center gap-2"><FileText className="size-[1em] shrink-0 text-muted-foreground" aria-hidden="true"/><h2 className="truncate text-sm font-semibold">{t("browser.document")}</h2></div>
                <Popover.Root open={filePickerOpen} onOpenChange={setFilePickerOpen}>
                  <Popover.Trigger render={<Button variant="outline" />} disabled={isDirectoryTreeLoading} aria-label={t("detail.chooseFile")} title={selectedRelativePath ?? "SKILL.md"} className="ml-auto min-w-0 w-72 max-w-[65%] justify-between">
                    <span className="min-w-0 truncate">{selectedRelativePath ?? "SKILL.md"}</span><ChevronDown className="shrink-0"/>
                  </Popover.Trigger>
                  <Popover.Portal>
                    <Popover.Positioner align="end" sideOffset={4} className="z-[60]">
                      <Popover.Popup aria-label={t("detail.chooseFile")} className="max-h-80 w-80 max-w-[calc(100vw-2rem)] overflow-auto rounded-md border border-border bg-popover p-3 text-popover-foreground shadow-md outline-none">
                        {directoryTree.map(node => <FileTreeNode key={node.path} node={node} level={0} selectedPath={selectedPreviewPath} expandedDirectories={expandedDirectories} onToggleDirectory={handleToggleDirectory} onSelectFile={file => {handleSelectFile(file);setFilePickerOpen(false);}}/>)}
                        {!directoryTree.length && <p className="text-sm text-muted-foreground">{t("detail.noFiles")}</p>}
                      </Popover.Popup>
                    </Popover.Positioner>
                  </Popover.Portal>
                </Popover.Root>
                {readingOpen ? <Button ref={readingCloseRef} variant="ghost" size="icon-sm" title={t("detail.closeReading")} aria-label={t("detail.closeReading")} onClick={closeReading}><X className="size-4" /></Button> : <Button ref={readingButtonRef} variant="ghost" size="icon-sm" title={t("detail.openReading")} aria-label={t("detail.openReading")} onClick={() => { readingOrigin.current = {top:previewScrollRef.current?.scrollTop ?? 0,left:previewScrollRef.current?.scrollLeft ?? 0}; setReadingOpen(true); }}><Maximize2 className="size-4" /></Button>}
              </div>}
              <div
                className={cn("space-y-4", variant === "inspector" ? "inspector-document p-3" : "p-6")}
                role="region"
                aria-label={t("detail.preview")}
              >
                {variant === "inspector" && directoryTreeError && <p role="alert" className="text-sm text-destructive">{directoryTreeError}</p>}
                {variant !== "inspector" && selectedRelativePath && (
                  <div className="text-xs font-mono text-muted-foreground break-all">
                    {selectedRelativePath}
                  </div>
                )}
                {isSelectedFileLoading ? (
                  <div className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Loader2 className="size-4 animate-spin" />
                    {t("common.loading")}
                  </div>
                ) : previewContent ? (
                  isSelectedMarkdownFile ? (
                    <>
                      <SkillFrontmatterCard data={frontmatterData} raw={frontmatterRaw} />
                      <div className={cn("markdown-body", detailTypographyClassName)}>
                        <ReactMarkdown remarkPlugins={[remarkGfm]}>
                          {markdownContent}
                        </ReactMarkdown>
                      </div>
                    </>
                  ) : (
                    <pre className="rounded-lg border border-border bg-card p-4 text-[12px] leading-5 font-mono whitespace-pre-wrap break-words text-foreground/80">
                      {previewContent}
                    </pre>
                  )
                ) : (
                  <p className="text-sm text-muted-foreground italic">
                    {t("detail.noContent")}
                  </p>
                )}
              </div>
            </div>

            <div
              role="separator"
              aria-label={t("detail.resizeSidebar")}
              aria-orientation="vertical"
              className={cn("hidden w-2 shrink-0 cursor-col-resize items-stretch justify-center border-l border-border/70 bg-background transition-colors hover:bg-primary/10", variant !== "inspector" && "md:flex")}
              onMouseDown={handleSidebarResizeStart}
              onDoubleClick={() => setDetailSidebarWidth(DEFAULT_DETAIL_SIDEBAR_WIDTH)}
            >
              <div className="my-auto h-10 w-0.5 rounded-full bg-border" />
            </div>

            {/* ── Right: Sidebar ─────────────────────────────────────── */}
            <aside
              data-testid="skill-detail-right-sidebar"
              hidden={variant === "inspector" && inspectorTab === "document"}
              style={sidebarStyle}
              className={cn("w-full border-border overflow-y-auto", variant === "inspector" ? "p-4 space-y-4" : "p-4 space-y-4", variant === "inspector" ? "min-h-0 flex-1" : "shrink-0 border-t md:w-[var(--skill-detail-sidebar-width)] md:border-t-0")}
            >
              {detail ? (
                <>
                  <section hidden={variant === "inspector"} aria-label={t("detail.filesRegion")}>
                    <SectionLabel>{t("detail.files")}</SectionLabel>
                    {isDirectoryTreeLoading ? (
                      <div className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Loader2 className="size-4 animate-spin" />
                        {t("common.loading")}
                      </div>
                    ) : directoryTreeError ? (
                      <p className="text-sm leading-6 text-muted-foreground">
                        {directoryTreeError}
                      </p>
                    ) : directoryTree.length > 0 ? (
                      <div className="space-y-1">
                        {directoryTree.map((node) => (
                          <FileTreeNode
                            key={node.path}
                            node={node}
                            level={0}
                            selectedPath={selectedPreviewPath}
                            expandedDirectories={expandedDirectories}
                            onToggleDirectory={handleToggleDirectory}
                            onSelectFile={handleSelectFile}
                          />
                        ))}
                      </div>
                    ) : (
                      <p className="text-sm text-muted-foreground">{t("detail.noFiles")}</p>
                    )}
                  </section>

                  {(detail.source_kind || detail.is_read_only) && (
                    <section
                      aria-label={t("detail.sourceStatusRegion", {
                        defaultValue: i18n.language.startsWith("zh") ? "来源状态" : "Source status",
                      })}
                    >
                      <SectionLabel>
                        {t("detail.sourceStatus", {
                          defaultValue: i18n.language.startsWith("zh") ? "来源状态" : "Source status",
                        })}
                      </SectionLabel>
                      <div className="rounded-lg border border-border/70 bg-background p-3 space-y-2">
                        <div className="flex flex-wrap items-center gap-2">
                          {detail.source_kind && (
                            <SourceOriginBadge originKind={detail.source_kind} />
                          )}
                          {detail.is_read_only && <ReadOnlySourceBadge />}
                        </div>
                        {detail.is_read_only ? (
                          <p className="text-sm leading-6 text-muted-foreground">
                            {t("detail.readOnlyDesc", {
                              defaultValue: i18n.language.startsWith("zh")
                                ? "只读观测副本仅供查看，不能在这里编辑备注、标签或调整技能集。"
                                : "Read-only observed copies are display-only here, so notes, tags, and collection changes are unavailable.",
                            })}
                          </p>
                        ) : null}
                      </div>
                    </section>
                  )}

                  <section hidden={variant === "inspector" && inspectorTab === "document"} aria-label={t("detail.metadataRegion")}>
                    <details open className="group/overview-section">
                    <summary className="overview-section-title"><Info className="size-4 shrink-0" aria-hidden="true"/>{t("detail.metadataRegion")}<ChevronRight className="size-4 shrink-0 group-open/overview-section:rotate-90" aria-hidden="true"/></summary>
                    <SectionPanel className="bg-transparent">
                      {canEditBasicSource ? (
                        <>
                          <div className="space-y-2">
                            <label className="text-sm font-normal text-muted-foreground">
                              {t("detail.sourceType")}
                            </label>
                            <select
                              value={sourceTypeInput}
                              onChange={(event) => setSourceTypeInput(event.target.value)}
                              className={cn(
                                "h-9 w-full rounded-md border border-input bg-background px-3 text-sm shadow-xs outline-none",
                                "focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
                              )}
                            >
                              {SOURCE_TYPE_OPTIONS.map((option) => (
                                <option key={option} value={option}>
                                  {option}
                                </option>
                              ))}
                            </select>
                          </div>
                          <div className="space-y-2">
                            <label className="text-sm font-normal text-muted-foreground">
                              {t("detail.sourceRepo")}
                            </label>
                            <Input
                              value={sourceRepoInput}
                              onChange={(event) => setSourceRepoInput(event.target.value)}
                              placeholder="owner/repo"
                              className="h-9 text-sm"
                            />
                          </div>
                          <div className="space-y-2">
                            <label className="text-sm font-normal text-muted-foreground">
                              {t("detail.sourcePath")}
                            </label>
                            <Input
                              value={sourcePathInput}
                              onChange={(event) => setSourcePathInput(event.target.value)}
                              placeholder="skills/example/SKILL.md"
                              className="h-9 text-sm"
                            />
                          </div>
                          <div className="space-y-2">
                            <label className="text-sm font-normal text-muted-foreground">
                              {t("detail.sourceUrl")}
                            </label>
                            <Input
                              value={sourceUrlInput}
                              onChange={(event) => setSourceUrlInput(event.target.value)}
                              placeholder="https://github.com/owner/repo"
                              className="h-9 text-sm"
                            />
                          </div>
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"

                            disabled={isSavingSourceMetadata}
                            onClick={handleSaveSourceMetadata}
                          >
                            {isSavingSourceMetadata ? (
                              <>
                                <Loader2 className="size-4 animate-spin" />
                                {t("detail.savingSourceMetadata")}
                              </>
                            ) : (
                              <><ActionIcon action="save"/>{t("detail.saveBasicInfo")}</>
                            )}
                          </Button>
                        </>
                      ) : (
                        <>
                          {!detail.source_kind && displayedSource && (
                            <MetadataRow label={t("detail.source")} value={displayedSource} href={sourceLinks.repository} directory={sourceLinks.repositoryDirectory} />
                          )}
                          {detail.source_repo && detail.source_repo !== displayedSource && (
                            <MetadataRow
                              label={t("detail.sourceRepo", {
                                defaultValue: i18n.language.startsWith("zh") ? "来源仓库" : "Source repository",
                              })}
                              value={detail.source_repo}
                              href={sourceLinks.repository} directory={sourceLinks.repositoryDirectory}
                            />
                          )}
                          {detail.source_path && (
                            <MetadataRow
                              label={t("detail.sourcePath", {
                                defaultValue: i18n.language.startsWith("zh") ? "来源路径" : "Source path",
                              })}
                              value={detail.source_path}
                              href={sourceLinks.sourcePath} directory={sourceLinks.sourceDirectory}
                            />
                          )}
                        </>
                      )}
                      {variant !== "inspector" && detail.created_at && (
                        <MetadataRow
                          label={t("detail.createdAt", {
                            defaultValue: i18n.language.startsWith("zh") ? "创建时间" : "Created",
                          })}
                          value={new Date(detail.created_at).toLocaleString()}
                        />
                      )}
                      {variant !== "inspector" && detail.updated_at && (
                        <MetadataRow
                          label={t("detail.updatedAt", {
                            defaultValue: i18n.language.startsWith("zh") ? "更新时间" : "Updated",
                          })}
                          value={new Date(detail.updated_at).toLocaleString()}
                        />
                      )}
                      {storageMetadataRows.map(([label, value]) => (
                        <MetadataRow key={`${label}:${value}`} label={label} value={value} directory />
                      ))}
                    </SectionPanel>
                                      </details>
                  </section>
                  {detail && !detail.is_read_only && (
                    <>
                    <section hidden={variant === "inspector" && inspectorTab === "document"} aria-label={t("detail.notesRegion")}>
                      <SectionLabel icon={<NotebookPen className="size-4 shrink-0" aria-hidden="true"/>}>{t("detail.notes")}</SectionLabel>
                      <div className="space-y-3 rounded-lg border border-border/70 bg-background p-3">
                        <div className="space-y-2">
                          <Textarea
                            value={notesInput}
                            onChange={(event) => setDraft("notes", event.target.value)}
                            placeholder={t("detail.notesPlaceholder")}
                            className="min-h-24 resize-y text-sm leading-6"
                          />
                        </div>
                        <div className="flex flex-wrap items-center justify-end gap-2">
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"

                            disabled={!skillContent || isExplanationLoading || isExplanationStreaming}
                            aria-label={t(isExplanationLoading||isExplanationStreaming ? "detail.explanationLoading" : "detail.generateNote")}
                            onClick={handleGenerateNote}
                          >
                            {isExplanationLoading || isExplanationStreaming ? (
                              <>
                                <Loader2 className="size-4 animate-spin" />
                                {t("detail.explanationLoading")}
                              </>
                            ) : (
                              <><ActionIcon action="ai"/>{t("workflow.ai")}</>
                            )}
                          </Button>
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"

                            disabled={isSavingMetadata}
                            onClick={handleSaveNotes}

                          >
                            {metadataAction === "notes-save" ? (
                              <>
                                <Loader2 className="size-4 animate-spin" />
                                {t("detail.savingMetadata")}
                              </>
                            ) : (
                              <><ActionIcon action="save"/>{t("common.save")}</>
                            )}
                          </Button>
                          <Button size="sm" variant="outline" disabled={isSavingMetadata} onClick={()=>void handleClear("notes")}>{metadataAction === "notes-clear" ? <Loader2 className="size-4 animate-spin" /> : <ActionIcon action="clear"/>}{t("workflow.clear")}</Button>
                        </div>
                        {isExplanationStreaming && explanation && (
                          <p className="flex items-center gap-2 text-xs text-muted-foreground">
                            <Loader2 className="size-4 animate-spin" />
                            {t("detail.explanationStreaming")}
                          </p>
                        )}
                        {explanationError && (
                          <div className="space-y-1 rounded-md border border-destructive/30 bg-destructive/5 p-2">
                            <p className="text-xs leading-5 text-destructive">
                              {explanationErrorInfo?.message || explanationError}
                            </p>
                            {explanationErrorInfo?.fallbackTried && (
                              <p className="text-xs leading-5 text-muted-foreground">
                                {t("detail.fallbackTried")}
                              </p>
                            )}
                          </div>
                        )}
                      </div>
                    </section>

                    <section hidden={variant === "inspector" && inspectorTab === "document"} aria-label={t("detail.tagsRegion")}>
                      <SectionLabel icon={<Tag className="size-4 shrink-0" aria-hidden="true"/>}>{t("detail.tags")}</SectionLabel>
                      <div className="space-y-3 rounded-lg border border-border/70 bg-background p-3">
                        <div className="space-y-2">
                          <Input
                            disabled={isGeneratingTags}
                            value={tagsInput}
                            onChange={(event) => setDraft("tags", event.target.value)}
                            placeholder={t("detail.tagsPlaceholder")}
                            className="h-9 text-sm"
                          />
                        </div>
                        <div className="flex flex-wrap items-center justify-end gap-2">
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"

                          disabled={isGeneratingTags || isSavingMetadata || !skillContent}
                          aria-label={t("detail.generateTags")}
                          onClick={handleGenerateTags}
                        >
                          {isGeneratingTags ? <Loader2 className="size-4 animate-spin" /> : <ActionIcon action="ai"/>}
                          {t("workflow.ai")}
                        </Button>
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"

                          disabled={isSavingMetadata || isGeneratingTags}
                          onClick={handleSaveTags}

                        >
                          {metadataAction === "tags-save" ? (
                            <>
                              <Loader2 className="size-4 animate-spin" />
                              {t("detail.savingMetadata")}
                            </>
                          ) : (
                            <><ActionIcon action="save"/>{t("common.save")}</>
                          )}
                        </Button>
                        <Button size="sm" variant="outline" disabled={isSavingMetadata} onClick={()=>void handleClear("tags")}>{metadataAction === "tags-clear" ? <Loader2 className="size-4 animate-spin" /> : <ActionIcon action="clear"/>}{t("workflow.clear")}</Button>
                        </div>
                      </div>
                    </section>
                    </>
                  )}



                  {variant === "inspector" && inspectorTab === "overview" && installationSummary}
                  {/* Skill Bundles */}
                  <section hidden={variant === "inspector" && inspectorTab === "document"} aria-label={t("detail.collections")}>
                    <SectionLabel icon={<Layers className="size-4 shrink-0" aria-hidden="true"/>}>{t("detail.collections")}</SectionLabel>
                    <SectionPanel>
                    {detail.is_read_only ? (
                      <p className="text-sm leading-6 text-muted-foreground">
                        {t("detail.readOnlyCollectionsBlocked", {
                          defaultValue: i18n.language.startsWith("zh")
                            ? "只读观测副本不可调整技能集。"
                            : "Bundle management is unavailable for read-only observed copies.",
                        })}
                      </p>
                    ) : (
                      <div className="flex flex-wrap gap-2 items-center">
                        {skillCollections.map((collection) => (
                          <span
                            key={collection.id}
                            className="inline-flex items-center gap-2 rounded-full bg-primary/10 px-2.5 py-1 text-xs font-medium text-primary ring-1 ring-primary/20"
                            title={collection.description ?? collection.name}
                          >
                            <Tag className="size-4" />
                            {collection.name}
                          </span>
                        ))}
                        <Button
                          ref={addToCollectionButtonRef}
                          variant="ghost"
                          size="sm"
                          className="text-muted-foreground hover:text-foreground"
                          aria-label={t("detail.addToCollection")}
                          onClick={() => setIsCollectionPickerOpen(true)}
                        >
                          <Plus className="size-4" />
                          {t("detail.addToCollection")}
                        </Button>
                      </div>
                    )}
                    </SectionPanel>
                  </section>
                </>
              ) : null}
            </aside>
          </div>
        )}
      </div>

      {/* Collection Picker Dialog */}
      {skillId && !detail?.is_read_only && (
        <CollectionPickerDialog
          open={isCollectionPickerOpen}
          onOpenChange={handleCollectionPickerOpenChange}
          skillId={skillId}
          currentCollectionIds={skillCollections.map((collection) => collection.id)}
          onAdded={handleCollectionAdded}
        />
      )}
    </div>
  );
}
