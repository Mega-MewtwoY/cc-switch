# CC Switch（Kimi Code 定制版）

> 本项目是 [farion1231/cc-switch](https://github.com/farion1231/cc-switch) 的 fork，在上游 v3.19.2 基础上增加了对 **Kimi Code CLI（`kimi`）** 的完整支持。上游项目的功能介绍请移步原仓库。

CC Switch 是一个用 Tauri 2 + React 编写的桌面应用，用于集中管理和切换各类 AI 编程工具的供应商配置，支持 Claude Code、Codex、Gemini CLI、OpenCode 等。本定制版在此基础上把 Kimi Code 作为第 9 个受管应用接入，并针对 Kimi 的 OAuth 订阅模式做了专门适配。

## 本定制版新增的功能

- **Kimi Code 应用标签页**：顶部应用切换器新增 "Kimi Code"，统一管理 `~/.kimi-code/config.toml` 中的供应商
- **内置 Moonshot 预设**：官方站（api.moonshot.cn）、国际站（api.moonshot.ai）、自定义，预填 kimi-k2.6 模型，填 Key 即可用
- **OAuth 供应商自动导入**：先用 `kimi` CLI 登录后，CC Switch 启动时自动识别并导入官方 OAuth 供应商，也可在供应商列表手动点"导入当前配置"
- **OAuth 令牌自动续期**：kimi 的 access_token 只有 15 分钟有效期，本版会在到期前用 refresh_token 自动续期并原子写回凭据文件，用量查询不再 401
- **Coding Plan 用量显示**：官方 OAuth 供应商开箱即用，无需配置脚本；主页供应商卡片页脚直接显示套餐用量，用量仪表盘提供 token / 费用统计（数据来源为本地会话日志解析）
- **会话管理**：Sessions 页面支持筛选、查看和删除 Kimi Code 的会话记录
- **MCP / Skills / Prompts**：支持管理 `~/.kimi-code/mcp.json`，MCP 服务器和 Skills 可按应用勾选启用
- **终端中打开**：供应商卡片一键打开终端并启动 `kimi`（macOS / Linux / Windows）
- **macOS Dock 行为修正**：关窗只隐藏窗口、保留 Dock 图标，点 Dock 图标可唤回窗口

## 使用说明

### 方式一：使用官方订阅（OAuth）

1. 在终端运行 `kimi`，按提示完成 OAuth 登录
2. 打开 CC Switch，切到 "Kimi Code" 标签页，官方供应商会自动出现在列表中（如未出现，点"导入当前配置"）
3. 供应商卡片页脚会直接显示 Coding Plan 套餐用量；令牌过期由应用自动续期，无需干预

### 方式二：使用 API Key

1. 在 "Kimi Code" 标签页点击"添加供应商"
2. 选择预设（Moonshot 官方 / 国际站），填入 API Key 和供应商标识（小写字母、数字、连字符）
3. 保存后点击"切换"，即写入 `config.toml` 的 `default_model` 生效

### 其他入口

- **用量统计**：切换到"用量"页面，可按 Kimi Code 过滤查看 token 消耗和费用
- **会话记录**：切换到 "Sessions" 页面，供应商筛选选择 Kimi Code
- **打开终端**：供应商卡片上的终端按钮会直接启动 `kimi`

## 安装

本 fork 目前没有独立的安装包分发渠道（上游的官网 / Homebrew 渠道安装的官方版不含 Kimi Code 功能），请从源码构建。

### 环境要求

- Node.js 18+
- pnpm 8+（仓库锁定 pnpm@10.12.3）
- Rust 1.85+ 与 Tauri CLI 2.8+
- 平台：Windows 10+ / macOS 12+ / Linux（Ubuntu 22.04+ 等）

### 构建步骤

```bash
git clone https://github.com/Mega-MewtwoY/cc-switch.git
cd cc-switch
pnpm install
pnpm build        # 产出平台安装包（macOS .dmg / Windows .msi / Linux .deb 等）
```

开发模式运行：

```bash
pnpm dev
```

常用检查命令：

```bash
pnpm typecheck          # 前端类型检查
pnpm test:unit          # 前端单元测试（vitest）
cd src-tauri && cargo test   # 后端测试
```

## 与上游的关系

- 基于上游 v3.19.2，会不定期合并上游更新
- 所有定制改动集中在 `kimicode` 相关模块，分叉点和合并策略记录在 [KIMICODE_SYNC.md](KIMICODE_SYNC.md)
- 与 Kimi Code 无关的问题请优先到[上游仓库](https://github.com/farion1231/cc-switch)反馈

## 许可证

MIT License，与上游一致。上游版权归 Jason Young 及贡献者所有。
