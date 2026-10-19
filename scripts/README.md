# 构建中间产物清理

三个平台入口均根据脚本位置定位仓库，可从任意工作目录调用。默认只列出清理范围；显式传入执行参数后才删除。

| 平台 | 预览 | 执行删除 |
| --- | --- | --- |
| Windows | `powershell -ExecutionPolicy Bypass -File scripts/clean-build-windows.ps1` | 在左侧命令后加 `-Run` |
| macOS | `bash scripts/clean-build-macos.sh` | 在左侧命令后加 `--run` |
| Linux | `bash scripts/clean-build-linux.sh` | 在左侧命令后加 `--run` |

## 清理范围

- Cargo 本机与交叉编译的 `debug`、`release` 中的 `.fingerprint`、`build`、`deps`、`incremental`，扫描仓库内 `src-tauri/target` 和 `target`。
- 前端 `dist`，Vite 的 `.vite`、`node_modules/.vite`、`node_modules/.vite-temp`。
- 根目录生成的 `tsconfig.tsbuildinfo`、`tsconfig.node.tsbuildinfo`、`vite.config.js`、`vite.config.d.ts`。

保留最终可执行文件、动态库、调试符号、`bundle` 安装包、`release-assets`、依赖目录主体、打包配置、源码及 `.arsenal` 应用数据。仓库外的自定义 Cargo 输出目录不处理；`tmp`、日志和测试报告不属于本次清理范围。

macOS 入口复用 Linux 入口的同一份 Bash 清理逻辑，适用于两者的本机和交叉编译目录，不依赖 GNU 专用工具。复制使用时请保留这两个脚本。

先结束构建和开发服务，再执行删除，避免输出目录正在被写入。删除缓存后，下一次编译会重新生成中间产物，耗时可能增加。Windows 脚本拒绝通过文件系统链接删除，也拒绝删除包含链接的目标目录；Bash 脚本拒绝目标及其父目录是符号链接的情况，递归删除不会跟随内部符号链接。

现有 `package-release-*` 的 `-Clean`/`--clean` 是独立的旧清理入口，范围更大，可能删除完整构建目录；本页的新入口仅用于清理中间产物，不调用旧清理逻辑。
