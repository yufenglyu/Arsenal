# Arsenal 命名规范

产品名称为 **Arsenal**，中文名称为“百器库”，产品说明为“AI 工具与能力管理”。界面不显示副标题，不增加 Hub 或 Manager 后缀。

源码、配置模板、文档、安装脚本和构建产物中的品牌标识统一使用 Arsenal / arsenal：

| 用途 | 标识 |
| --- | --- |
| 配置目录 | `.arsenal` |
| 返回仓库的协议 | `arsenal://repository/<UUID>?tab=notebook` |
| Obsidian 稳定笔记链接 | `obsidian://arsenal-note?id=<UUID>` |
| 笔记属性键 | `arsenal_*` |
| 笔记模板设置键 | `arsenal_property_template` |
| Obsidian 插件 ID / 安装目录 | `arsenal-link` |
| 浏览器缓存、拖放类型、备份文件名 | `arsenal` 前缀 |

旧品牌的属性、协议、插件、缓存键与模板设置不提供别名、回退读取、兼容或自动迁移。旧配置和笔记不自动转换，也不自动改写或删除用户已有文件；需要时重新配置模板、关联笔记并安装当前插件。

存储修改通过 Store 提交，安装链接通过 linker 接口维护，不直接修改内容寻址对象。应用 identifier 与安装包升级 GUID 保持独立管理，不因普通品牌文字整理重建。
