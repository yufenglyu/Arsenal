// Documentation-only fixture. Renders production components with fictional data.
// Vite rejects screenshot-demo modules in production builds. Never imports user data.

import {createRoot} from "react-dom/client";
import {MemoryRouter} from "react-router-dom";
import App from "../../src/App";
import "../../src/index.css";
import i18n from "../../src/i18n";
import {InstallDialog} from "../../src/components/central/InstallDialog";
import {useResourceLibraryStore as library} from "../../src/stores/resourceLibraryStore";
import {usePlatformStore as platforms} from "../../src/stores/platformStore";
import {useCentralSkillsStore as central} from "../../src/stores/centralSkillsStore";
import {useCollectionStore as groups} from "../../src/stores/collectionStore";
import {useSettingsStore as settings} from "../../src/stores/settingsStore";
import {useSkillDetailStore as details} from "../../src/stores/skillDetailStore";
import {useRepositoryDocumentStore as documents} from "../../src/stores/repositoryDocumentStore";
import {useNotebookStore as notebooks} from "../../src/stores/notebookStore";
import {useMetadataStore as metadata} from "../../src/stores/metadataStore";
import {useTaskQueueStore as queue, type BackgroundTask} from "../../src/stores/taskQueueStore";
import {useRepositorySyncStore as sync} from "../../src/stores/repositorySyncStore";
import {useShortcutStore} from "../../src/stores/shortcutStore";
import {DEFAULT_SHORTCUTS} from "../../src/lib/shortcutKeys";
import type {AgentWithStatus, SkillWithLinks, RepositorySyncPreview} from "../../src/types";

if (!import.meta.env.DEV) throw new Error("Documentation fixture is development-only");
const params = new URLSearchParams(location.search);
const en = params.get("lang") === "en";
const scene = params.get("scene") ?? "library";
const tr = (zh: string, english: string) => en ? english : zh;
await i18n.changeLanguage(en ? "en" : "zh");
localStorage.clear();
document.documentElement.dataset.theme = params.get("theme") ?? "light";
document.documentElement.dataset.accent = "blue";
localStorage.setItem("skillshub.browser.width", "42");
localStorage.setItem("skills-manage.skillTableColumns.tree.v5", JSON.stringify(["index","name","skillCount","updatedAt"]));
localStorage.setItem("skills-manage.skillTableColumns.tree.v5.notebook", "1");
localStorage.setItem("arsenal.browser.tab", JSON.stringify(scene === "document" ? "document" : "overview"));
localStorage.setItem("skillshub.browser.tab", JSON.stringify(scene === "document" ? "document" : "overview"));
const noop = async () => {};
const date = "2026-10-19T08:00:00Z";
const agents: AgentWithStatus[] = [
  ["claude-code", "Claude Code", "~/.claude/skills"], ["codex", "Codex", "~/.agents/skills"],
  ["cursor", "Cursor", "~/.cursor/skills"], ["gemini-cli", "Gemini CLI", "~/.gemini/skills"],
  ["project:1", "Research Studio", "~/Projects/research/.agents/skills"],
  ["project:2", "Website", "~/Projects/website/.agents/skills"],
].map(([id, display_name, global_skills_dir]) => ({id, display_name, global_skills_dir, is_detected:true, is_builtin:!id.startsWith("project:"), is_enabled:true,shares_central_skills:id==="codex"}));
const definitions = [
  ["research-kit", "literature-review", "梳理文献、比较证据并整理研究空白", "Compare sources, synthesize evidence and identify research gaps", "研究", "Research"],
  ["research-kit", "paper-search", "构建检索式，按主题汇总论文", "Build search queries and collect papers by topic", "检索", "Search"],
  ["research-kit", "citation-check", "核对引用出处与参考文献信息", "Verify citations and bibliography metadata", "研究", "Research"],
  ["research-kit", "academic-writing", "组织论证结构，润色学术表达", "Structure arguments and refine academic writing", "写作", "Writing"],
  ["dev-toolkit", "code-review", "检查变更风险，给出可执行的修复建议", "Review changes and suggest actionable fixes", "开发", "Development"],
  ["dev-toolkit", "test-design", "从需求与边界条件设计测试", "Design tests around requirements and edge cases", "测试", "Testing"],
  ["dev-toolkit", "api-design", "梳理接口契约与错误处理", "Define API contracts and error handling", "开发", "Development"],
  ["dev-toolkit", "release-notes", "把提交记录整理成面向用户的更新说明", "Turn commits into user-facing release notes", "写作", "Writing"],
  ["design-studio", "interface-audit", "检查信息层级、交互状态和可访问性", "Audit hierarchy, interaction states and accessibility", "设计", "Design"],
  ["design-studio", "design-tokens", "整理颜色、字体和间距规范", "Organize colors, typography and spacing", "设计", "Design"],
  ["writing-desk", "technical-writing", "撰写清晰的技术说明与操作指南", "Write clear technical explanations and guides", "写作", "Writing"],
  ["writing-desk", "copy-editing", "精简冗余表达，保持一致的语气", "Remove repetition and maintain a consistent voice", "写作", "Writing"],
];
const skills: SkillWithLinks[] = definitions.map(([repo,name,zh,english,tagZh,tagEn],index) => ({
  id:name, name, description:tr(zh,english), source:"github", source_author:"example", source_repo:`example/${repo}`,
  source_url:`https://github.com/example/${repo}`, source_path:`${name}/SKILL.md`,
  file_path:`~/.arsenal/library/example/${repo}/${name}/SKILL.md`, canonical_path:`~/.arsenal/library/example/${repo}/${name}`,
  is_central:index<4, scanned_at:date,created_at:"2026-10-12T08:00:00Z", updated_at:date,
  linked_agents:index<4?["claude-code","project:1"]:index<8?["cursor","codex"]:[],read_only_agents:[],
  tags:[tr(tagZh,tagEn),tr("常用","Favorite")],notes:tr("先整理问题与输入材料，再开始执行。输出后核对关键结论与引用。","Define the question and collect inputs first. Verify key findings and citations after execution."),
}));
library.setState({skills,agents,resourceLibraryDir:"~/.arsenal/library",loadResourceLibrary:noop});
platforms.setState({agents,skillsByAgent:{"claude-code":4,codex:4,cursor:4,"gemini-cli":0,"project:1":4,"project:2":0},initialize:noop,refreshCounts:noop});
central.setState({skills:skills.slice(0,4),agents,loadCentralSkills:noop,loadCentralBundles:noop});
const collections = [
  {id:"research",name:tr("研究工作","Research"),parent_id:null},
  {id:"papers",name:tr("论文写作","Paper writing"),parent_id:"research"},
  {id:"dev",name:tr("软件开发","Development"),parent_id:null},
  {id:"design",name:tr("产品设计","Product design"),parent_id:null},
  {id:"writing",name:tr("写作与表达","Writing"),parent_id:null},
].map(row=>({...row,created_at:date,updated_at:date}));
groups.setState({collections,groupDetails:collections.map((row,index)=>({...row,skills:skills.filter((_,i)=>index===0?i<4:index===1?i===3:index===2?i>=4&&i<8:index===3?i>=8&&i<10:i>=10)})),loadCollections:noop,loadGroupDetails:noop,refreshCounts:noop});
settings.setState({scanDirectories:[{id:1,is_builtin:false,added_at:date,path:"~/Projects/research",label:"Research Studio",is_active:true},{id:2,is_builtin:false,added_at:date,path:"~/Projects/website",label:"Website",is_active:true}],loadScanDirectories:noop});
useShortcutStore.setState({shortcuts:{...DEFAULT_SHORTCUTS}});
metadata.setState({loadFolders:noop,folders:[{id:"research-note",skillIds:skills.slice(0,4).map(skill=>skill.id),notes:tr("用于选题、文献检索与论文写作。先用 paper-search 收集资料，再用 literature-review 比较方法与结论。","For research planning, literature discovery and academic writing. Start with paper-search, then compare methods and findings with literature-review.")}]});
notebooks.setState({load:noop,loadConfig:noop,notebookNavigation:async()=>()=>{}});
const content = tr("# 文献综述\n\n围绕一个明确的研究问题，建立可以追溯的证据清单。\n\n## 适用场景\n\n- 开题前比较已有研究方法\n- 汇总多个来源的共识与分歧\n- 为论文撰写相关工作章节\n\n## 工作步骤\n\n1. 明确研究问题、时间范围和纳入标准。\n2. 整理来源，记录方法、数据与结论。\n3. 比较证据强度，标注限制和研究空白。\n4. 输出综述提纲与待核实清单。\n\n## 输出格式\n\n| 内容 | 要求 |\n| --- | --- |\n| 研究脉络 | 按主题组织，而非逐篇复述 |\n| 证据表 | 保留出处与限制 |\n| 后续问题 | 区分事实与推测 |\n\n> 示例技能文档，仅用于展示阅读界面。", "# Literature review\n\nBuild a traceable evidence map around a clear research question.\n\n## When to use\n\n- Compare methods before starting a study\n- Summarize agreement and disagreement across sources\n- Prepare the related-work section of a paper\n\n## Workflow\n\n1. Define the question, date range and inclusion criteria.\n2. Record methods, datasets and findings for each source.\n3. Compare evidence quality and identify limitations.\n4. Produce an outline and a verification checklist.\n\n## Deliverables\n\n| Output | Requirement |\n| --- | --- |\n| Research map | Organize by theme |\n| Evidence table | Keep sources and limitations |\n| Open questions | Separate facts from assumptions |\n\n> Example skill document for interface demonstration.");
details.setState({loadDetail:async request=>{
  const skill = skills.find(s=>s.id===(typeof request==="string"?request:request.skillId)) ?? skills[0];
  details.setState({detail:{...skill,installations:skill.linked_agents.map(agent_id=>({skill_id:skill.id,agent_id,installed_path:`~/.agents/skills/${skill.name}`,link_type:"symlink"})),collections:[collections[0]]},content,isLoading:false});
},loadCachedExplanation:noop});
documents.setState({readDocuments:async()=>[{name:"README.md",language:en?"en":"zh",content}],readReadme:async()=>content});
// Explicit local IPC substitute: read-only fixture values; never calls the desktop bridge.
Object.assign(window,{__TAURI_INTERNALS__:{invoke:async(command:string,args:Record<string,unknown>={})=>{
  if(command==="list_skill_directory") return [{name:"SKILL.md",path:`${args.dirPath}/SKILL.md`,relative_path:"SKILL.md",is_dir:false,children:[]}];
  if(command==="read_file_by_path") return content;
  if(command==="list_repository_snapshot_cache") return [];
  if(command==="get_platform_icons") return {};
  throw new Error(`Unsupported documentation action: ${command}`);
}}});
const repo: RepositorySyncPreview = {repository:"example/research-kit",snapshotId:"demo-review",remoteRef:"demo-commit",added:[{skillId:"evidence-map",name:"evidence-map",files:[{path:"SKILL.md",status:"added"}]}],modified:[{skillId:"literature-review",name:"literature-review",files:[{path:"SKILL.md",status:"modified"},{path:"references/checklist.md",status:"added"}]}],deleted:[],unchanged:[{skillId:"paper-search",name:"paper-search"},{skillId:"citation-check",name:"citation-check"}],documentChanges:[{path:"README.md",status:"modified"}]};
sync.setState({preview:{repositories:[repo,{repository:"example/dev-toolkit",added:[],modified:[],deleted:[],unchanged:[],error:"NETWORK_ERROR"},{repository:"example/design-studio",added:[],modified:[],deleted:[],unchanged:[{skillId:"design-tokens",name:"design-tokens"}],remoteRef:"demo-clean"}]},open:["tasks","review","import"].includes(scene),centerView:scene==="review"?"pending":"history",reportCheckedAt:Date.now(),repositories:null,reportGeneration:"demo",applied:false});
const taskBase = {cancelRequested:false,createdAt:Date.now(),locks:[]};
const prepared: BackgroundTask = {...taskBase,id:"import-demo",key:"import-demo",kind:"import",label:tr("导入技能","Import skills"),status:"awaiting_input",steps:[{command:"prepare_github_resource_import",label:"example/writing-desk",args:{repoUrl:"https://github.com/example/writing-desk"},status:"success",result:{repo:{owner:"example",repo:"writing-desk",branch:"main",normalizedUrl:"https://github.com/example/writing-desk"},skills:["technical-writing","copy-editing","meeting-notes"].map((name,index)=>({sourcePath:`${name}/SKILL.md`,skillId:name,skillName:name,rootDirectory:"",skillDirectoryName:name,downloadUrl:"",...(index===0?{conflict:{existingSkillId:name,existingName:name,proposedSkillId:name,proposedName:name}}:{})}))}}]};
queue.setState({tasks:[prepared,{...taskBase,id:"refresh-demo",key:"refresh-demo",kind:"check",label:tr("刷新仓库","Refresh repository"),status:"running",steps:[{command:"refresh_repository",label:"example/design-studio",args:{repository:"example/design-studio"},status:"running"}]}]});
if (scene === "review") queue.setState({tasks:[]});
createRoot(document.getElementById("root")!).render(<MemoryRouter initialEntries={["/resources"]}><App/>{scene==="install"&&<InstallDialog open onOpenChange={()=>{}} skill={skills[4]} agents={agents} onInstall={noop}/>}</MemoryRouter>);
