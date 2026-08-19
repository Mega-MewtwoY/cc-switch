# Kimi Code Harness 维护指南

本分支（`feat/kimicode`）在 CC Switch 上游基础上新增 Kimi Code CLI（kimi）支持。
本文档说明如何在官方更新后同步，保证本改动不被覆盖。

## 仓库结构

- `origin` → 你的 fork（首次推送前请在 GitHub 上 fork farion1231/cc-switch，
  然后执行 `git remote set-url origin git@github.com:<你的用户名>/cc-switch.git`）
- `upstream` → 官方仓库 farion1231/cc-switch（只读跟踪）
- `main` → 跟踪上游，不要直接在 main 上改动
- `feat/kimicode` → 全部 kimicode 改动所在分支

## 同步官方更新（推荐流程）

```bash
git checkout main
git pull upstream main          # 或 git fetch upstream && git merge upstream/main
git push origin main

git checkout feat/kimicode
git merge main                  # 或 git rebase main（历史更干净，但已推送的分支慎用）

# 验证
pnpm typecheck && pnpm test:unit
cd src-tauri && cargo check && cargo test
```

## 为什么冲突会很少

1. **核心逻辑都在新文件**，官方更新永远不会碰到：
   - `src-tauri/src/kimicode_config.rs`（TOML 读写核心）
   - `src-tauri/src/kimicode_mcp.rs`（~/.kimi-code/mcp.json 读写）
   - `src-tauri/src/mcp/kimicode.rs`（MCP 同步/导入）
   - `src-tauri/src/services/session_usage_kimicode.rs`（wire.jsonl usage.record 用量同步）
   - `src-tauri/src/session_manager/providers/kimicode.rs`（会话扫描/消息/删除）
   - `src/config/kimicodeProviderPresets.ts`（供应商预设）
2. **对已有文件的修改全是"追加式"**：枚举加变体、match 加分支、数组加元素。
   即使官方在同一区域也有改动，git 大多能自动合并；手工解冲突时也只需
   "两边都保留"。
3. **编译器是安全网**：上游若改了 `AppType` 相关接口，merge 后 `cargo check`
   会直接列出需要补分支的位置（本项目刻意使用穷尽 match）。

## 同步后必查清单

- [ ] `cargo check` 无 `non-exhaustive patterns` 错误（新 AppType 分支被重构时最常见）
- [ ] `cargo test` 全绿（重点：`kimicode_config::tests`）
- [ ] `pnpm typecheck && pnpm test:unit` 全绿
- [ ] 上游若新增了 harness（对比 `AppType` 变体数），参照其新分支检查 kimicode 是否也需要

## 长期建议

如果功能稳定，考虑向官方提 PR（先开 issue 说明意愿）。官方近几个版本持续
合入新 harness（hermes、openclaw、grokbuild），接受度较高。合入上游后本指南
即可作废，直接跟随官方版本。

## 有意的行为分叉（合并上游时需留意）

以下修改不是纯追加，而是改变了上游原有行为，merge 冲突时按本分支语义保留：

1. **macOS 关窗行为**（`src-tauri/src/lib.rs` `CloseRequested`）：
   去掉了关闭时 `apply_tray_policy(handle, false)`，点叉只隐藏窗口、
   保留 Dock 图标与运行点，经 `RunEvent::Reopen` 唤回。
   配套修改：单实例回调显示窗口时补 `apply_tray_policy(app, true)`，
   否则静默启动隐藏的 Dock 图标永远不会恢复。
2. **终端启动器抽取**（`src-tauri/src/commands/misc.rs`）：
   从 claude 启动路径中抽出了 `launch_macos_script_in_terminal` /
   `launch_linux_script_in_terminal` 公共函数。上游若改这两段
   （终端分发/终端列表），解冲突时把上游改动套回抽取后的公共函数。
3. **`ProviderService::current`**（`services/provider/mod.rs`）：
   KimiCode 是累加模式里唯一保留"当前供应商"语义的应用，
   该函数对 KimiCode 例外，不返回空串。
4. **official 供应商的 KimiCode 例外**（检测联通 / 用量查询）：
   上游对 `category == "official"` 的供应商隐藏检测与用量配置按钮、
   且 `stream_check` 拒绝解析 base_url（封号保护）。KimiCode 的官方
   供应商是自动导入的本地 OAuth 配置，base_url 在嵌套路径
   `settings_config.provider.base_url`，需要保留检测与用量入口：
   - `services/stream_check.rs` `resolve_base_url`：official 早退
     对 `AppType::KimiCode` 豁免（有回归测试
     `kimicode_official_provider_resolves_nested_base_url`）。
   - `src/components/providers/ProviderCard.tsx`：onTest /
     onConfigureUsage 的 official 判定均加 `appId === "kimicode"` 例外；
     卡片用量查询（`useUsageQuery` 的 `!isOfficial` 门）与页脚渲染分支
     （official 且无官方订阅模板时渲染 null）同样加 kimicode 例外，
     否则官方供应商的用量不会显示在主页卡片上。
   上游若改这两处判定逻辑，解冲突时保留 kimicode 例外。

## Phase 2 接入点（MCP / Skills / 用量 / 会话）

以下均为追加式修改，上游新增 harness 时对照检查。
注意：上游 v3.20.0 起新增了 Pi harness（同为 additive 模式；无 MCP、
无 current provider、prompts 走 SQLite）。代码中大量位置是 `pi` 分支与
`kimicode` 分支并列出现，解冲突时通常"两边都保留"。

- **数据库 schema v18**（`database/schema.rs`）：`mcp_servers` / `skills`
  各加 `enabled_kimicode` 列。上游 v3.20.0 已占用 v17（session_usage_dedup
  去重账本），故本迁移编号为 `migrate_v17_to_v18`，带 `table_exists` 守卫
  （同 v14→v15 模式）；CREATE TABLE 同步更新。若上游今后再次占用同一版本
  号，按同样方式处理：保留上游版本号的迁移，把 kimicode 列迁移顺延一位
  （两步操作均幂等，任意先后顺序的旧库都能安全升级）。
- **`app_config.rs`**：`McpApps` / `SkillApps` 加 `kimicode` 字段
  （serde default，旧配置兼容）。
- **MCP**：`dao/mcp.rs` SELECT/INSERT/column match；`services/mcp.rs`
  同步/删除/导入分发臂 + upsert 的 prev_apps 取消勾选处理 +
  `import_from_all_apps` 数组；前端 `McpFormModal` 复选框 +
  `appConfig.tsx` 的 `SKILLS_APP_IDS`（`MCP_APP_IDS` 复用它）。
- **Skills**：`dao/skills.rs` 全部 SQL；同步/扫描本身是通用的
  （`AppType::all()` + `get_skill_dir`，Phase 1 已接目录）。
- **用量**：`services/session_usage.rs` 的 `sync_all_unlocked` 加
  Kimi Code 步骤；`usage_stats.rs` 的 provider 命名 CASE、
  session/proxy 去重清单、cache_creation 容差清单；
  前端 `types/usage.ts` 的 `AppType`/`KNOWN_APP_TYPES`、
  `UsageDashboard`/`UsageHero` 的主题映射。
  用量脚本（Coding Plan 等）：后端 `provider.rs`
  `resolve_usage_credentials` 的 KimiCode 分支与前端
  `UsageScriptModal.tsx` `getProviderCredentials` 的 kimicode
  分支互为镜像，都读 `settings_config.provider.{base_url,api_key}`，
  改动须两边同步（漏前端会导致测试报 Unknown coding plan provider）。
  OAuth 令牌：官方供应商 `provider.api_key` 为空，令牌由 kimi CLI
  存在 `<kimi_dir>/credentials/kimi-code.json`；
  `kimicode_config::load_or_refresh_oauth_access_token` 读取并按需
  续期（access_token 寿命仅 15 分钟，kimi CLI 不在前台时不会刷新；
  端点 `https://auth.kimi.com/api/oauth/token` + 公开 client_id，
  续期成功原子写回凭据文件，写回前重读防覆盖 CLI 的轮换）；
  `coding_plan::get_coding_plan_quota` 的 Kimi 分支在 api_key
  为空时调用该函数（前端测试与后台轮询共用此路径）。
  kimi 的 `inputOther` 是 fresh input（Anthropic 风格），
  **不要**加入 `CACHE_INCLUSIVE_APP_TYPES`。
- **会话**：`session_manager/mod.rs` 的 scan 线程组（9 元组，
  kimicode=h8 / pi=h9，join 与 sessions.extend 同步增减）、
  load_messages / delete / provider_roots 分发；
  前端 `SessionManagerPage` 的 `ProviderFilter` + 下拉项，
  `App.tsx` 的 `hasSessionSupport` 清单与 sessions 视图的
  回退守卫（两处都要加 kimicode，漏掉回退守卫会导致
  点开会话页立即被弹回 providers）。
  会话 source_path 是**目录**（非文件），删除走 `remove_dir_all`。
