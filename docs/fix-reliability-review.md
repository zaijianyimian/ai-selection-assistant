# 修复变更代码审查与 OpenSpec Verify

日期：2026-10-16。变更：fix-settings-result-and-deb-reliability。本报告对应新增检索功能实施前的 0.1.2 本地检查点；Release 因用户新增需求而暂缓。

| 维度 | 结论 |
|---|---|
| 完整性 | 21/21 任务完成，9 条 ADDED 要求均有实现 |
| 正确性 | 23 个验收场景已按实现、单元测试、原生集成或发布验收流程核对 |
| 一致性 | 遵循暂存/事务/补偿、草稿基线、Job 元数据、最终 ELF 验包四项设计 |

## 要求与证据

| 规格要求 | 实现位置 | 验证证据 |
|---|---|---|
| Recoverable settings commit preserves memory state | src-tauri/src/settings_commit.rs:133；memory.rs:185 | every_precommit_failure_preserves_configuration_profiles_and_external_resources；原生快捷键冲突保留画像 |
| Successful commits retain stale task protection | lib.rs:95；settings_commit.rs；memory.rs | first_save_failure_removes_published_file_and_success_invalidates_old_writes；重开 SQLite、旧 record/finish 拒绝 |
| Compensation failure has explicit recovery state | settings_commit.rs:63；analysis.rs；lib.rs | 补偿失败、同资源重试、重启时原开关比较测试；原生模型请求门控 |
| External refresh preserves a dirty settings draft | SettingsView.vue:18 | 干净/脏表单、Key/删除意图测试；原生双窗口同步 |
| Explicit reload and local save establish a new baseline | SettingsView.vue:29；assistant.ts:66 | 主动重载、事件先/后于响应、失败草稿、明确提交保留草稿测试 |
| Translation results retain request display options | lib.rs:44、294；TranslationView.vue:13 | Job 序列化、进行中改语言和术语开关、两个视图组件测试 |
| Snapshot metadata is synchronized with the task | assistant.ts:17；assistant.test.ts | revision 乱序/恢复；原生主窗口与悬浮窗语言一致 |
| DEB declares executable runtime version requirements | scripts/linux_deb.py:23、44、58 | 正向包通过；三种弱依赖/缺工具/缺元数据负向检查；Ubuntu 22.04 拒绝配置 |
| Published compatibility is tied to the artifact | README.md；docs/verification.md；打包脚本 | Ubuntu 26.04 实装/启动记录、源码与二进制范围分开、未测范围明确记录 |

23 个场景分别归入上表九条要求；源规格保留完整 WHEN/THEN。当前无 REMOVED、RENAMED 或 MODIFIED 要求。

## 代码审查结果

未发现剩余阻断级问题。审查过程中补强了两个恢复边界：保留失败补偿的快捷键撤销信息；文件恢复失败后重启，用原记忆开关判断是否应失效画像。测试使用同一资源 fixture 验证恢复，避免新建假资源掩盖残留状态。设置提交后的注销/通知告警与未提交错误分开。

恢复标记仅含非敏感开关与快捷键撤销信息，凭据快照只在内存；没有增加原文/译文持久化。新增代码复用项目的 AppResult、数据库 actor、单调 Job revision 和 Vue/Pinia 模式。

## 实际检查

- 前端 23 项、Rust 31 项（隔离 X11，0 ignored）通过。
- 原生 WebKit WebDriver 16 项、DEB 正负向 6 项通过；中文截图已检查。
- Ubuntu 26.04 干净容器实际安装成功、普通用户进程存活及主窗口存在；22.04 libc6 2.35 不满足 >=2.39，包配置被拒绝。
- Vue/TypeScript/Vite、Rust fmt、git diff --check、OpenSpec 严格校验通过。
- 外部真实模型、用户日常凭据、Wayland 跨应用、Windows 二进制、多屏/硬件和全天调度未测；这些不作为已通过项。

跨文件系统、SQLite、凭据服务不承诺断电原子性，符合设计明确的边界。本报告针对修复变更，后续功能必须单独规划与验收；未执行归档。
