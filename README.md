# 2notes

> 不丢想法，不漏事务，不怕混乱。

2notes 是一个 Windows 桌面端的个人灵感与事务收集箱。它不是传统笔记软件，也不是完整待办系统；它更像一个安静待命的捕获入口：想到什么，先丢进去，稍后再澄清、处理、归档或导出。

![2notes product demo](docs/assets/2notes-demo.svg)

## 它解决什么

很多想法和待办不是在“准备写笔记”的时候出现的，而是在写代码、聊天、查资料、开会、复制链接的间隙突然冒出来。2notes 的第一目标是把这些碎片稳稳接住：

- 全局快捷键呼出快速记录，不打断当前工作。
- 自动保存草稿和详情编辑，尽量不让内容因为窗口关闭、切换或退出丢掉。
- 每条记录保留“原始内容 + 当前内容”，后续整理不覆盖最初的想法。
- 用类型、状态和标签做轻量澄清，不把收集箱变成复杂管理系统。
- 将未进入回收站的记录导出为 Markdown，数据始终在本机，能迁移、能备份、能带走。

## Demo：一次典型使用

1. 正在做别的事，按 `Ctrl + Alt + Space` 呼出快速记录。
2. 输入一句想法、一个待办或一段链接备注，按 `Enter` 保存。
3. 内容进入收集箱，默认是“未澄清 / 待处理”。
4. 回头在主窗口里补标题、改类型、打标签、标记完成或归档。
5. 需要迁移时，在设置页导出未进入回收站的记录；每条导出记录生成一个 `.md` 文件，带 YAML frontmatter 和原始内容追溯区。

## 当前能力

- 快速记录窗口：快捷键呼出、Enter 提交、Esc 隐藏、单份草稿恢复。
- 主窗口：收集箱、搜索、标签、回收站、设置。
- 知识库：同一条目可直接沉淀或移出知识库，不复制正文；支持唯一标题、历史标题别名、单父层级、面包屑和大纲。
- 结构化编辑：正文以稳定块 ID 保存，支持常用块、行内样式、撤销重做与自动保存；搜索、链接和导出从块文档派生。
- WikiLink：正文支持 `[[标题]]` 补全，并展示出链、反向链接及未解析链接。
- 知识健康：集中查看孤立条目、未解析链接等问题并跳转处理。
- 条目模型：标题、原始内容、当前内容、类型、状态、标签、修订号。
- 自动保存：详情编辑 debounce 保存，切换条目/视图/退出前会先 flush。
- 搜索筛选：标题、当前内容、原始内容、标签和历史标题；三字及以上查询使用 trigram FTS 排名与安全片段，一到两字按词回退 LIKE。
- 回收站：普通删除先移入回收站，永久删除只允许在回收站内发生。
- Markdown 导入与导出：导入会创建新条目，预览不支持内容的降级 warning，重复来源会跳过；导出仅包含未进入回收站的记录，文件名自动清理和避让，不覆盖已有文件。
- 本地优先：SQLite 是唯一内容真相源；Markdown 导入和导出均为显式复制，不与 SQLite 形成双主存储。
- Windows 集成：系统托盘、开机自启动开关、全局快捷键、安装包构建。

## 构建产物

运行：

```bash
pnpm install
pnpm run tauri:build
```

构建完成后会生成：

- 应用可执行文件：`src-tauri/target/release/two_notes.exe`
- MSI 安装包：`src-tauri/target/release/bundle/msi/2notes_<version>_x64_en-US.msi`
- NSIS 安装包：`src-tauri/target/release/bundle/nsis/2notes_<version>_x64-setup.exe`

这些产物不会提交到仓库；GitHub Actions 会在 `tauri-build` job 中验证真实打包链路。

## 开发

```bash
pnpm install
pnpm run tauri:dev
```

常用检查：

```bash
pnpm run lint
pnpm run typecheck
pnpm run test:unit
pnpm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
```

技术栈：

- 前端：Vue 3、TypeScript、Pinia、UnoCSS、VueUse
- 桌面端：Tauri 2
- 本地能力：Rust
- 存储：SQLite / rusqlite
- 包管理：pnpm

## 数据与隐私

2notes 当前是个人本地工具。主动记录、草稿、标签和设置都保存在本机 SQLite 中；Markdown 导出也只写入用户选择的本地目录。

默认数据路径由 Tauri app data 目录管理，设置页会展示数据目录和日志目录，并提供打开入口。

## 项目状态

第三阶段已完成块级本地知识库实现：稳定块 ID 与结构化编辑、主窗口新建、知识树与大纲、知识健康、Markdown 预览与导入，以及旧内容迁移到块文档。快速捕获和不丢内容仍是基础链路。

本地自动门禁与发布前证据需要分开看待：

- 常规 CI（push / pull request）验证前端、Rust 和 Tauri 打包链路。
- 手动触发的 [performance workflow](.github/workflows/performance.yml) 在 Windows release 模式运行块文档规模预算；常规 push / pull request CI 不执行高成本的 10,000 条目测试。
- Markdown 导入人工 fixture 位于 [`scripts/test/markdown-import-fixtures`](scripts/test/markdown-import-fixtures)，覆盖 2notes 导出、YAML 元数据、WikiLinks、HTML 降级 warning 和空文件。
- 真实 Windows 安装包中的快捷捕获、结构化编辑、导入导出、备份恢复和升级冒烟，必须在本地实际安装后留存记录；它不是 CI 或源码级测试的替代品。

常用验证：

- `pnpm run format:check`
- `pnpm run lint`
- `pnpm run typecheck`
- `pnpm run test:unit`
- `pnpm run build`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test/knowledge-scale.ps1`
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test/block-document-scale.ps1`
- `pnpm run tauri:build`

## 暂不做

当前仍不做图谱、AI、云同步、多人协作、插件、MCP、剪贴板流水、Markdown 双主存储、附件和富文本。先保持“快速捕获 + 不丢内容 + 本地知识闭环”简单可靠。
