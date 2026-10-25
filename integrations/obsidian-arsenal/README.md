# Arsenal Link

插件版本：0.2.0。显示名称为 Arsenal Link；安装目录与插件 ID 保留 `skillshub-link`，笔记属性和协议保留 `skillshub_*` / `skillshub://`，以兼容已有笔记与链接。

按稳定笔记 UUID 打开 Obsidian 笔记，文件改名、移动后保留相同链接。插件不写笔记、不开网络服务、不读取 Arsenal 数据库。

## 随应用打包

执行 `pnpm tauri build`、三平台 `package:release:*` 或 CI 发布构建时，会自动安装插件锁定依赖、构建插件、运行插件测试，并在应用发布目录生成 `arsenal_<应用版本>_obsidian-plugin.zip`。ZIP 只包含 `skillshub-link/` 下的 `main.js`、`manifest.json` 和本说明。

也可单独执行 `pnpm package:obsidian`；使用 `--output-dir <目录>` 指定输出位置。应用版本用于 ZIP 文件名，插件自身版本仍由其 manifest/package.json 管理。`SkipBuild` 和仅更新版本的模式不会生成插件包；跳过安装或测试的选项同步作用于插件。

解压后将整个 `skillshub-link` 文件夹放入库的 `.obsidian/plugins/`，再手动启用插件。打包不会安装到或读取任何 Obsidian 库。

## 安装

1. 运行 `pnpm install`、`pnpm build`（在本目录）；发行包已包含编译后的 `main.js`。
2. 将 `manifest.json`、`main.js` 放到需要联动的 Obsidian 库中的 `.obsidian/plugins/skillshub-link/`。
3. 在 Obsidian 的第三方插件设置中手动启用 **Arsenal Link**。插件未上架社区插件市场。
4. 在 Arsenal 中访问一次已有的关联笔记，或创建/关联新笔记，初始化 `skillshub_note_id`。

请在每台设备上安装并启用插件。笔记文件和 UUID 可以同步，本机库目录仍需在 Arsenal 中分别配置。

## 使用

稳定链接格式：`obsidian://skillshub-note?id=<笔记UUID>`，不包含路径、文件名、盘符或库名。

- Arsenal：最右侧关联图标菜单 → **复制链接**。
- Obsidian 命令面板：**Arsenal Link: Copy stable note link / 复制稳定笔记链接**。
- Obsidian 命令面板：**Arsenal Link: Open repository in Arsenal / 返回 Arsenal 仓库**。
- 出现外部变更后可使用 **Rebuild note index / 重建笔记索引** 命令。

Arsenal 自身的“在 Obsidian 中打开”先按 UUID 解析本机位置，再调用原生 Obsidian URI，因此无需此插件。复制出的固定 URI 则需要插件处理。

## 边界

- 插件只检索收到 URI 的当前 Obsidian 库。请先打开对应库；当多个库打开、当前库不同或目标库未加载时，固定 URI 不保证自动切换到正确库。无路径链接不包含库位置，插件也不读取 Obsidian 的其他库注册信息。
- 一次最多索引 20000 个 Markdown 文件；单文件上限 4 MiB。监听创建、修改、删除及改名，缓存索引不写入笔记或同步目录。
- 重复笔记 UUID 会明确报错，不随机选择。复制笔记时保留 ID 会产生歧义，需要先处理副本；不要删除原关联笔记的 ID。
- 协议仅打开已有笔记，不按 URI 创建、删除或修改文件。返回 Arsenal 的 URI 从有效仓库 UUID 构造，不执行任意属性中的 URL。
- Windows 正式版 Arsenal 启动时自动注册或修复 `skillshub://`，便携文件移动后先从新位置启动一次；多个副本以最近启动的正式版为准。macOS 应使用安装到 `/Applications` 的正式应用。

## 开发验证

`pnpm build` 运行官方 Obsidian 类型检查并生成 CommonJS 插件。`pnpm test` 在模拟 Obsidian API 的 Node 环境中验证冷启动等待、改名移动、重复/缺失 ID 和安全返回链接；这些测试不代表真实 Obsidian 协议验收。

插件使用官方 [`Plugin.registerObsidianProtocolHandler`](https://github.com/obsidianmd/obsidian-api) API。
