# 更新日志

本文件记录 2notes 的版本变更。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [0.3.0] - 2026-08-07

`package.json`、`tauri.conf.json` 和 `Cargo.toml` 中的版本号已是 `0.3.0`，待打 `v0.3.0` 标签；发布时由 `scripts/test/check-version.ps1` 校验三者与标签一致。

### 新增

- 块级知识库：正文以稳定块 ID 保存，支持常用块、行内样式、撤销重做与自动保存；搜索、链接和导出均从块文档派生。
- 知识文档父子层级、知识树面包屑与大纲。
- 知识健康工作区：集中查看孤立条目与未解析链接并跳转处理。
- WikiLink：正文 `[[标题]]` 补全，展示出链、反向链接与未解析链接。
- trigram 全文索引：三字及以上查询使用 FTS 排名与安全片段，一到两字按词回退 LIKE。
- Markdown 预览与导入：导入创建新条目，不支持的内容降级为 warning，重复来源跳过。
- 主窗口直接新建条目。
- 自动每日备份与备份恢复，恢复前校验完整性、外键与 schema，并保留回滚副本。

### 变更

- 旧的纯文本正文迁移到块文档，并在启动时修复投影。
- 安装包目标收敛为仅 NSIS，不再产出 MSI。
- `参考项目/` 加入 `.gitignore`（此前仅在本机 `.git/info/exclude` 中排除，克隆到其他机器会误提交约 2400 个文件）。
- 读操作改为连接池并发执行，写命令异步化，界面不再被数据库查询/写入阻塞。
- 列表页与知识补全改为索引化查询，标签计数收敛为相关子查询，避免大库全表扫描。
- 备份/恢复锁改为 RAII 守卫，失败或中断后不会再永久锁死；快速记录相关命令补齐窗口守卫。

### 修复

- 收紧自动保存排空与生命周期语义，保持详情自动保存修订链一致。
- 统一删除保存边界并稳定搜索输入。
- 串行化设置写入并按字段回滚，避免部分应用。
- 收敛备份、设置与草稿的并发与事务语义。
- 修复令牌缺失、front-matter 误吞、导出文件名无界、标签规范化回写。
- 防止健康摘要失效后被旧响应覆盖；标签刷新只应用最新结果。
- 稳定首屏主题并满足深色模式对比度要求。
- 条目列表键盘导航改为按 id 定位，不再在每次方向键时遍历所有已渲染行。

### 工程

- CI 在 Windows 上验证前端（format/lint/typecheck/单测/build/依赖公告）、Rust（fmt/test/clippy/cargo-audit）与 Tauri 打包链路。
- 发布流程改为「先建草稿 → 已安装 NSIS 包全链路冒烟 → 通过后才 publish」。
- 新增 Tauri capability 白名单与 command guard 检查脚本。
- `knowledge-scale.ps1` 接入 performance workflow（此前无任何 workflow 引用）。
- 新增 Dependabot 配置，覆盖 npm、cargo 与 GitHub Actions。
- PR 层新增源码 EXE 的 CDP 冒烟与 runner 自测，发布专用的 E2E 脚本在每次推送即回归。
- `check-version.ps1` 改为基于脚本位置解析仓库根，并强制 CHANGELOG 存在对应版本节。

## [0.2.0] - 2026-07-01

### 新增

- 快速记录提交链路，配合事件驱动的列表刷新。

## [0.1.0] - 2026-07-01

### 新增

- 首个 Windows 桌面版本：全局快捷键快速捕获、收集箱、搜索、标签、回收站与设置。
- 条目模型保留原始内容与当前内容，整理不覆盖最初的想法。
- 详情编辑 debounce 自动保存，切换条目、视图与退出前先 flush。
- SQLite 作为唯一内容真相源，Markdown 导出为显式复制。
- Windows 集成：系统托盘、开机自启动开关、NSIS 安装包。

[未发布]: https://github.com/242282218/2notes/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/242282218/2notes/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/242282218/2notes/releases/tag/v0.1.0
