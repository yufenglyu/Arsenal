# JSON 存储实施验证记录

配套：[方案与阶段门槛](JSON存储改造方案与实施计划.md)。仅记录已执行结果，未执行项不作通过结论。测试数据均为临时目录中的合成数据，不收集用户数据。

## P0：契约与基线

日期：2026-10-17；运行平台：Windows；基于规划提交 6d3ad11 及当前工作区。工作区存在原有文档、图标、界面和打包修改，因此结果不是干净 HEAD 的独立发行版证明；本次提交不纳入这些修改。

### 验证结果

| 命令 | 结果 |
| --- | --- |
| `cargo test --lib --quiet` | 544 passed，0 failed，5 ignored；31.97s |
| `pnpm test` | 80 文件、813 passed；14.61s |
| `pnpm typecheck` | 通过 |
| `pnpm lint` | 通过 |
| `pnpm build` | 通过；存在大于 500 kB 的 bundle 提示 |

5 项原有 ignored 测试依赖公共 GitHub 网络/大型仓库/隔离重放环境，本次未执行，不归入通过。新增性能基准也是显式 opt-in，不替换功能测试。

### 存储调用清单

| 模块 | 需要替换的依赖/行为 | 主要回归测试位置 |
| --- | --- | --- |
| db.rs | 结构定义、初始化、68 个公开查询/变更方法、SQL executor 事务入口 | db.rs tests |
| lib.rs | AppState、启动初始化、旧目录迁移、恢复顺序、SQL 插件 | 启动/协议相关测试及 P7 桌面 |
| config_store.rs | 根据连接池推导配置路径、进程内锁、未知字段保留 | config_store.rs tests |
| path_utils.rs / settings.rs | 目录指针、便携路径、数据库 sidecar、目录切换 | path_utils.rs/settings.rs tests |
| agents.rs | 平台配置投影、共享中心、启停、项目目标 | agents.rs tests |
| scanner.rs | 逐项 upsert、观测、过期记录清理、扫描范围 | scanner.rs tests |
| linker.rs | 安装记录、归属、symlink/copy、重定位 | linker.rs tests |
| skills.rs | 本地添加/批量导入/移除、路径和来源关系 | skills.rs tests |
| collections.rs | 集合成员和批量安装 | collections.rs tests |
| github_import.rs | 事务 executor、提交凭据、文件事务恢复 | github_import.rs tests |
| remote_sources.rs | 解释缓存、来源查询和更新 | remote_sources.rs tests |
| metadata.rs | 延迟 folder_notes 表、标签批量事务 | metadata.rs tests |
| notebook_repository.rs / notebook.rs | 延迟笔记表、整数 ID/UUID/绑定、恢复、内部正文并发 | 两模块 tests |
| repository_address.rs | 仓库地址/目录/链接/笔记联合迁移 | repository_mirror.rs 地址迁移测试 |
| repository_mirror.rs | 快照/摘要/状态、恢复 payload、禁止清理未完成操作 | repository_mirror.rs tests |
| backup.rs | 逻辑导出/恢复、来源及安装映射、笔记与范围控制 | backup.rs tests |
| build/prepare_config_dir.rs | 打包时默认目录生成，不能遗留建库 | db.rs prepare_config_dir 测试及打包 |
| Cargo/package/capabilities | 直接及传递依赖、Tauri SQL 插件权限 | P6 依赖树、P7 构建 |

### 必须保持的行为契约

1. `upsert_skill`：已有 central 记录遇到非 central 重扫时保留名称/正文/路径/来源，不降级；缺省 canonical_path 保留已有值；更新扫描时间。
2. 资源库查询条件是非 central 且 canonical_path 非空，并按名称排序。JSON 必须复现条件及显式排序；无 ORDER BY 的查询不承诺偶然的 SQL 返回顺序。
3. 托管安装更新不改 created_at；扫描发现不能覆盖已有托管记录的路径、链接和归属。持久模型必须包含 is_managed。
4. 启动计算项目序号下限，考虑安装和观测中已有 project ID，配置重置不能导致身份复用。
5. 技能删除存在不同范围的入口；保留各自的关联清理语义，不能统一成删除所有相关资源。中心移除不删资源库原文件。
6. 来源检查时间更新不覆盖已安装 remote_ref、fingerprint 或 last_sync_at。
7. 标签保留现有 trim/长度约束和大小写比较；仓库 SQL NOCASE、路径比较不能用同一规则替代。
8. 笔记 restore 可重分配本地整数 ID，但保留 repository_uid 和 source_key；唯一绑定检查、正文长度和 expected_body 冲突检查保持。
9. 镜像地址迁移保持成员 ID、集合、标签、笔记和安装链接；存在冲突或未恢复事务时禁止覆盖目标。
10. 备份 included 范围和 WebDAV 仓库范围保持；备份不是全部内部事务状态的替代导出。
11. IPC DTO、字段命名、空值和错误映射保持；内部持久类型与 DTO 分离。
12. 不把未完成/失败扫描范围当作空集合清理。部分操作失败保持已有错误展示与可恢复状态。

### 性能基准说明

运行：`cargo test --lib storage_baseline -- --ignored --nocapture`。

夹具：1k/10k/50k 技能，每技能一条托管平台安装，每十技能一条集合成员；无真实磁盘技能树，无网络。事务批量建数，每种查询/编辑采样 20 次。时间为 debug 测试配置，不是发行版承诺。重开指标是连接重开及首次查询，不是完整应用冷启动；数据库文件大小不等于写入字节数。

本基准覆盖存储基本查询、编辑和批量灌入；全量文件扫描、导入恢复、备份、峰值内存、大正文、真实冷启动及断电测试在后续相应阶段补齐，不能从当前结果推断。

无并行构建的复测结果（时间单位 μs，批量灌入为 ms）：

| 技能数 | 批量灌入 ms | ID 查询 p95 | 平台全量 p95 | 集合 p95 | 备注保存 p95 | 重开首次查询 | DB 字节 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1,000 | 39 | 205 | 9,352 | 982 | 1,779 | 1,223 | 487,424 |
| 10,000 | 398 | 483 | 93,626 | 9,703 | 2,136 | 1,226 | 3,481,600 |
| 50,000 | 2,829 | 569 | 629,090 | 110,499 | 2,176 | 3,008 | 17,326,080 |

全量平台返回的记录数随档位增长，包含映射成本；不能与单条查询等价比较。测试过程中没有改变生产存储实现。

## P1：解除耦合（进行中）

第一步：将 10 个领域 DTO 提取到 storage/models.rs，去掉模型上的 SQLx derive，SQLite 行解码留在 db.rs。保留原 db 路径重导出，避免 IPC 模型变化。config_store 的生产实现不再导入数据库类型；新增 ConfigStore 显式目录入口，旧连接池路径推导隔离到 db_config.rs 过渡适配层。

验证：`cargo test --lib --quiet` 为 545 passed、0 failed、6 ignored（其中 1 项为新增 opt-in 基准）；`cargo clippy --lib -- -D warnings` 通过。新增无数据库配置读写/重开/目录隔离测试。未修改生产存储格式。

P1 尚未完成：命令模块中的直接 SQL、完整持久记录及事务接口仍需逐域收拢；现有调用通过过渡适配层仍使用 SQLite。P2–P7 尚未实施。
