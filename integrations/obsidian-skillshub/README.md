# SkillsHub Link

插件版本：0.2.0。本目录由 GitHot Link 迁移，供当前 SkillsHub 仓库笔记功能使用。

按稳定笔记 UUID 打开 Obsidian 笔记，文件改名、移动后保留相同链接。插件不写笔记、不开网络服务、不读取 SkillsHub 数据库。

## 安装

1. 运行 `pnpm install`、`pnpm build`（在本目录）；发行包已包含编译后的 `main.js`。
2. 将 `manifest.json`、`main.js` 放到需要联动的 Obsidian 库中的 `.obsidian/plugins/skillshub-link/`。
3. 在 Obsidian 的第三方插件设置中手动启用 **SkillsHub Link**。插件未上架社区插件市场。
4. 在 SkillsHub 中访问一次已有的关联笔记，或创建/关联新笔记，初始化 `skillshub_note_id`。

请在每台设备上安装并启用插件。笔记文件和 UUID 可以同步，本机库目录仍需在 SkillsHub 中分别配置。

## 使用

稳定链接格式：`obsidian://skillshub-note?id=<笔记UUID>`，不包含路径、文件名、盘符或库名。

- SkillsHub：最右侧关联图标菜单 → **复制链接**。
- Obsidian 命令面板：**SkillsHub Link: Copy stable note link / 复制稳定笔记链接**。
- Obsidian 命令面板：**SkillsHub Link: Open repository in SkillsHub / 返回 SkillsHub 仓库**。
- 出现外部变更后可使用 **Rebuild note index / 重建笔记索引** 命令。

SkillsHub 自身的“在 Obsidian 中打开”先按 UUID 解析本机位置，再调用原生 Obsidian URI，因此无需此插件。复制出的固定 URI 则需要插件处理。

## 边界

- 插件只检索收到 URI 的当前 Obsidian 库。请先打开对应库；当多个库打开、当前库不同或目标库未加载时，固定 URI 不保证自动切换到正确库。无路径链接不包含库位置，插件也不读取 Obsidian 的其他库注册信息。
- 一次最多索引 20000 个 Markdown 文件；单文件上限 4 MiB。监听创建、修改、删除及改名，缓存索引不写入笔记或同步目录。
- 重复笔记 UUID 会明确报错，不随机选择。复制笔记时保留 ID 会产生歧义，需要先处理副本；不要删除原关联笔记的 ID。
- 协议仅打开已有笔记，不按 URI 创建、删除或修改文件。返回 SkillsHub 的 URI 从有效仓库 UUID 构造，不执行任意属性中的 URL。
- Windows 正式版 SkillsHub 启动时自动注册或修复 `skillshub://`，便携文件移动后先从新位置启动一次；多个副本以最近启动的正式版为准。macOS 应使用安装到 `/Applications` 的正式应用。

## 开发验证

`pnpm build` 运行官方 Obsidian 类型检查并生成 CommonJS 插件。`pnpm test` 在模拟 Obsidian API 的 Node 环境中验证冷启动等待、改名移动、重复/缺失 ID 和安全返回链接；这些测试不代表真实 Obsidian 协议验收。

插件使用官方 [`Plugin.registerObsidianProtocolHandler`](https://github.com/obsidianmd/obsidian-api) API。
