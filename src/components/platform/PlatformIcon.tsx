import { usePlatformIconStore } from "@/stores/platformIconStore";
import Claude from "@lobehub/icons/es/Claude/components/Mono";
import OpenAI from "@lobehub/icons/es/OpenAI/components/Mono";
import Cursor from "@lobehub/icons/es/Cursor/components/Mono";
import GithubCopilot from "@lobehub/icons/es/GithubCopilot/components/Mono";
import Gemini from "@lobehub/icons/es/Gemini/components/Mono";
import Cline from "@lobehub/icons/es/Cline/components/Mono";
import Kimi from "@lobehub/icons/es/Kimi/components/Mono";
import Trae from "@lobehub/icons/es/Trae/components/Mono";
import Qwen from "@lobehub/icons/es/Qwen/components/Mono";
import Windsurf from "@lobehub/icons/es/Windsurf/components/Mono";
import Qoder from "@lobehub/icons/es/Qoder/components/Mono";
import OpenCode from "@lobehub/icons/es/OpenCode/components/Mono";
import Antigravity from "@lobehub/icons/es/Antigravity/components/Mono";
import { Cpu, FolderOpen } from "lucide-react";
import { isProjectAgentId } from "@/lib/projectTargets";
import { cn } from "@/lib/utils";

interface PlatformIconProps {
  agentId: string;
  brand?: boolean;
  displayName?: string;
  className?: string;
  /** Icon size in pixels (default: 14). */
  size?: number;
}

const brandIcons = {
  "claude-code": Claude, codex: OpenAI, "codex-cli": OpenAI, cursor: Cursor,
  copilot: GithubCopilot, "github-copilot": GithubCopilot, "gemini-cli": Gemini,
  cline: Cline, "kimi-code-cli": Kimi, trae: Trae, "trae-cn": Trae, qwen: Qwen,
  windsurf: Windsurf, qoder: Qoder, opencode: OpenCode, antigravity: Antigravity,
};
const abbreviations: Record<string, string> = { warp: "W", workbuddy: "WB", codebuddy: "CB", kiro: "K", pi: "π", "codearts-agent": "CA", hermes: "H", openclaw: "OC" };

export function PlatformIcon({ agentId, className, size = 14, brand = false, displayName }: PlatformIconProps) {
  const icon = usePlatformIconStore(state => state.icons[agentId]);
  if (icon && !isProjectAgentId(agentId)) return <img src={icon} alt="" width={size} height={size} className={cn("shrink-0 rounded-sm bg-white object-contain p-px", className)} />;
  if (brand && !isProjectAgentId(agentId)) {
    const BrandIcon = brandIcons[agentId as keyof typeof brandIcons];
    if (BrandIcon) return <BrandIcon size={size} className={cn("shrink-0", className)} aria-hidden />;
    const initials = abbreviations[agentId] ?? (displayName ?? agentId).slice(0, 2).toUpperCase();
    return <span aria-hidden className={cn("inline-flex shrink-0 items-center justify-center rounded border border-current text-[9px] font-semibold leading-none", className)} style={{width:size,height:size}}>{initials}</span>;
  }
  const Icon = isProjectAgentId(agentId) ? FolderOpen : Cpu;

  return (
    <Icon
      width={size}
      height={size}
      className={cn("shrink-0", className)}
      aria-hidden
    />
  );
}
