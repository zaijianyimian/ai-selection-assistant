# Design

## Context

动机和范围见 [proposal.md](proposal.md)，行为契约见 [specs/token-usage/spec.md](specs/token-usage/spec.md)。本设计需要创建，因为采集跨越共用 HTTP 层、Job、分析状态、本地持久化和两个窗口，并涉及故障、清空及重试的一致性。

已核对的实现入口：

- `src-tauri/src/llm.rs`：`complete/perform/send` 共用非流式 Chat Completions；`perform` 内进行格式和 Temperature 兼容降级，外层 timeout 覆盖整个逻辑请求。当前只返回 `(content, fallback)`，且在检查截断/拒绝之前未采集 usage。
- `src-tauri/src/lib.rs`：`run_job` 与 `test_connection` 使用同一翻译传输；前者可能先从 `cache::Cache` 返回，`Job` 的 revision 与请求选项由后端拥有。
- `src-tauri/src/analysis.rs`：手动和自动分析共用 `request -> llm::complete`，与翻译可并行；分析返回后还可能因为画像校验或隐私 epoch 改变而失败。
- `src-tauri/src/cache.rs`：仅进程内缓存 Translation。不能把原始请求用量塞进此对象后在命中时重播。
- 现有 rusqlite bundled 已提供本地存储；术语库存在 v1/v2 同步迁移，统计不应增加对此库、记忆开关或同步服务的耦合。
- 当前没有 usage 类型、统计 IPC、UI 或历史数值。已有规格不要求保存数字统计，但要求保护 Key、原文和模型响应；本变更新增独立能力并保持这些边界。

参考：已核对 [DeepSeek Chat Completions usage 字段](https://api-docs.deepseek.com/api/create-chat-completion/)（2026-10-17）。这里仅以返回字段为依据，不引用或固化模型价格、最大上下文值或账户余额接口；兼容服务仍可不提供 usage。

## Goals / Non-Goals

**Goals:**

- 为每个实际尝试建立稳定身份，保留已观察到的数字，并使窗口和汇总共享同一后端事实。
- 即使结果失败、进程中断或统计存储异常，也避免把“不知道”展示成零或完整账单。
- 累计统计可跨重启保留、独立清空，并在 Windows/Linux 上保持一致行为。

**Non-Goals:**

- 不追溯旧版本消耗，不覆盖同 Key 在其他应用/设备的消耗。
- 不查余额、报价或估算金额，不设置 Token 硬上限，不增加 tokenizer、模型或统计联网任务。
- 不改同步协议，不让统计写入/修复成为翻译、设置恢复或术语迁移的新前置门禁。

## Decisions

### 1. 在共用传输层注入观察器，分开 operation 与 attempt

新增独立 `usage` 模块及可在测试中替换的计量观察器。翻译、连接测试、手动分析和自动分析入口创建 operation UUID 及不可变上下文；`perform` 每次真正调用 `send` 前生成新的 attempt UUID。上下文固定用途、规范化服务标识、请求 Model、UTC 发起时间、本机日期/时区偏移和统计 generation，不携带 Key、原文或提示词。

```
translation / connection-test / manual-analysis / auto-analysis
                              |
                              v
                  operation + captured context
                              |
                 +------------+-------------+
                 |                          |
            local cache                HTTP attempt(s)
                 |                          |
            new usage = 0          validated usage / unknown
                 |                          |
                 +------------+-------------+
                              |
                 per-operation snapshot + local aggregates
```

每个响应在 HTTP 状态处理、兼容重试和结果解析之前检查顶层 `usage`。HTTP 错误即使需要重试，只要有有效 usage 也保留；缺失则终结为未知。后续截断、拒绝、缺少 content、模型 JSON 解析失败或隐私检查失败均不能撤回已经观察到的用量。前端事件仅通知读快照，不产生记账写入。

超时/取消使用有终结保证的 attempt ticket；无法取得完整响应、超过现有 1 MiB 上限等保持未知，不为寻找 usage 绕过响应限制。没有开始 HTTP 的校验、凭据、busy 或恢复失败不创建 attempt。

替代方案：只在翻译成功后计数会漏掉测试、分析和失败；在组件计数会因双窗口、事件重播和后台分析重复或漏算；因此不采用。

### 2. 数据值与覆盖程度一起建模

使用可选的非负整数 input/output/total、cached-input 和 reasoning-output 字段，另带来源与质量状态。缺失、null、负数、小数、越界或冲突均转为受影响字段的未知，不能导致译文失败；不保留任何原始字段字符串或错误正文。

- 标准映射：`prompt_tokens`、`completion_tokens`、`total_tokens`。
- 明细映射：`prompt_tokens_details.cached_tokens` / `prompt_cache_hit_tokens` 是同一个输入子集；一致时只取一次，不一致时该明细未知。`completion_tokens_details.reasoning_tokens` 为输出子集。`prompt_cache_miss_tokens` 可用于一致性检查，不再加到输入父计数上。
- 父计数存在时，明细不得大于父计数；明细异常不丢弃有效父计数。若 input/output/total 都有值但不一致，则 total 未知，仍保留独立有效的 input/output。
- 缺失 total 不偷偷补成“服务报告总量”。汇总分别维护各字段已知合计、已知/未知尝试数；显示“已报告合计，另有 N 次未完整报告”。无历史、明确零、未知和待完成状态分开。
- 整数解析和累计使用 checked 64 位非负计数；溢出保留旧值并标记不完整。IPC 中 Token 计数用十进制字符串或 null，前端用 BigInt/字符串格式化，避免超过 JavaScript 安全整数后悄悄失真。

operation 的展示为各尝试的已知合计加覆盖标记，不把“最终一次响应的 total”误当作整次重试消耗，也不凭字符数或模型名称推算未知值。质量问题采用固定代码，不能日志打印服务商原始字段。

替代方案：必填数值默认 0 会制造完整性假象；将所有明细相加会重复计数；引入 tokenizer 只能估算且影响包体，均不采用。

### 3. 快照归属与缓存分开

新增可选的 operation 用量快照到 Job、ConnectionTest 和 AnalysisStatus，而非 Translation。成功与失败均通过快照展示；主窗口/浮窗沿用后端 revision 门禁，统计查询/事件再使用 usage generation/revision，丢弃迟到快照。

`run_job` 本地缓存命中直接产生 `local-cache` 快照：本次新增 input/output/total 为 0，attempt 数为 0，不写 provider 累计，也不重复记录缓存生成时的用量。强制刷新走正常 observer；清空翻译缓存不清空统计。连接测试、分析都保持原有真实请求语义。

同一服务更换 API Key 后仍归入该服务/模型的本应用统计，不能假称按账户统计。服务标识用规范化 endpoint 的稳定摘要区分不同网关路径，界面仅用经校验的主机/端口标签及已配置模型；不保存完整 URL、凭据或凭据摘要，也不直接保存响应中的任意 model 文本。服务身份方案不依赖缓存用于隔离 Key 的进程内指纹。

替代方案：把 usage 与 Translation 一同缓存容易重播旧消耗；读取当前全局设置归类会误记在途请求，均不采用。

### 4. 独立 SQLite，事务性终结及按日汇总

在 app_data_dir 新增 `usage.sqlite3`（独立 schema v1），由单工作线程/有界队列串行访问；不在模型 HTTP 等待中持有数据库事务，也不持有 settings/术语锁写统计。利用已有 SQLite；新建目录/文件沿用 Unix 0700/0600 和 Windows 当前用户目录权限，不添加凭据回退文件。

建议逻辑表：

| 表 | 用途与必要数据 |
|---|---|
| metadata | schema、generation、累计起点、revision、完整性状态 |
| attempts | attempt/operation UUID、generation、用途、服务摘要/标签、已配置 Model、UTC/本机日期/偏移、pending/final、可选数字及质量标记 |
| daily_usage | generation＋日期＋服务＋模型＋用途的已知数字合计、字段覆盖计数与尝试数 |

`begin` 登记 pending；`finish` 在同一事务中从 pending 变为 final 并更新 daily_usage，重复 finish 为空操作。正常完成、错误、超时都终结一次。启动时把已登记但未终结的旧尝试按未知终结，不重发请求。已终结收据仅保留最近 7 天用于幂等，pending 不按时间误删；日汇总继续保留并支持累计，不依赖长期逐请求明细重建。模型/用途分组查询参数化并分页（默认 20、最多 100 组），避免无限返回。

今日/本月/累计由日汇总查询。归属日期以每次 HTTP 尝试发起时设备本机日历为准，同时保存 UTC 和偏移；兼容重试跨日时各自归属各自开始日期。后续改时区不回溯重排，累计不受影响。实现时注入时钟/日期提供器测试，不在并发测试中修改全局 TZ。

替代方案：复用 memory.sqlite3 会让清空术语、禁用记忆、同步迁移/备份影响统计；仅内存累计无法跨重启；永久逐请求明细不符合聚合优先的目标，均不采用。

### 5. 清空以 generation 隔离旧请求

设置页提供“清空用量统计”，二次确认具体范围并说明“不会取消已发送的模型请求；清空前发起的请求不进入新的累计”。确认时原子递增 generation、重置累计起点并删除当前统计/收据；旧 ticket 的 finish 和迟到窗口事件无法重建旧数据。新请求使用新 generation。

清空不删除当前可见译文、翻译缓存、术语、画像、模型设置、同步状态或 Key。旧请求仍可显示其本次观测数字，并注明未纳入清空后的统计。取消确认没有写入；关闭窗口或刷新页面也没有清空/计数副作用。

替代方案：仅将汇总置零无法阻止正在返回的请求回填，因此不采用。

### 6. 统计不可用属于非致命故障

observer 使用有界提交/等待并受原有整体请求 deadline 约束；不让磁盘慢或队列满形成无限等待。统计开始/终结失败不阻止 HTTP 或有效结果，当前 operation 的内存快照仍可展示观察到的数字，累计标记“统计未完整保存/暂不可用”。不为保存统计重新请求模型，不补猜丢失数量。

DB 打不开、损坏、磁盘满或未来 schema 只禁用统计写入并提供静态错误/重试读取；不得自动删除重建文件，也不改变原有 settings 恢复门禁。恢复可用后只能重试已有数字事件（保持 attempt identity/generation），不能重放模型请求。无已知可靠完整性时不得显示“完整账单”或静默重置为零。

替代方案：统计故障向上传播成翻译失败会迫使用户重试并产生额外请求，因此不采用。

### 7. 展示与说明

- TranslationView：主窗口和浮窗都在结果/错误附近显示本次 input/output/total 与覆盖提示；本地缓存显示“本次新增 0 Token”。控制字号与换行，保留滚动和复制/刷新按钮。
- SettingsView：连接测试消息保留用量；独立 UsageSettings 卡片显示今日、本月、累计、起算时间、分类/覆盖信息、存储警告、清空与说明，不混入“保存模型配置”表单的事务。
- MemoryView：展示最近手动或自动分析的本次用量，不因失败保留旧画像而把新分析消耗丢掉。
- 说明明确 Token 不等于字/字符，输入还包含固定提示词及启用后的个性化摘要，输出包含结构化解释/模型报告的推理明细；服务端缓存依然是模型请求，本地命中才不产生新的模型请求。
- 区分本机已报告用量、单次输出限制/上下文长度、账户配额和金额；不显示“剩余余额”“精确节省 Token”或根据未知值推导价格。介绍只读取本地文案/数据，不联网。

替代方案：只在成功译文末尾放一个 total 无法解释额外分析、失败及未知覆盖，因此不采用。

## Risks / Trade-offs

- 部分服务或失败响应没有 usage → 已知数字是观测下界，显示未知覆盖，不宣称全账户准确账单。
- 请求确已发出但进程在记账前异常退出 → 对已记录 pending 标记未知；无法完整持久化时披露缺口，不重发也不补算。
- 历史本机日期与后来时区不同 → 保留发生时归属并在说明中写明规则，用注入时钟覆盖午夜、月界及夏令时。
- clear 与并行分析/翻译竞争、双窗口事件乱序 → generation＋attempt 幂等事务＋后端 revision，测试迟到结果与重复终结。
- SQLite/整数/兼容字段异常 → 单字段质量标记、checked 累计、统计非致命降级；保持 HTTP/响应体/隐私限制。
- 跨平台文件权限及落盘差异 → 复用本项目近期 Windows 备份刷盘修复经验，只对可写文件句柄做刷盘；在 Windows CI 与 Linux 实际执行重启、恢复和清空测试。
- 短期收据与日汇总增加本地元数据 → 不保存内容，完成收据 7 天后清理，分组查询分页；首版不增加逐请求日志浏览功能。

## Migration Plan

1. 实现时新增独立统计库及可选 IPC 字段，首次使用从零开始并显示起算时间；不能从术语出现次数、历史画像或缓存内容反推过去消耗。
2. 不迁移/降级记忆数据库，不改变现有模型配置结构、Key 或同步协议；统计不自动进入术语备份/同步载荷。
3. 旧版本回退后忽略独立统计文件，现有翻译/术语可继续使用。新版本遇到未知的更高 usage schema 停止统计写入并提示，不破坏文件。
4. 后续实现完成后执行 Rust、前端、原生本地模拟服务、Windows 原生 CI、DEB 验包及 Windows EXE/便携 ZIP 的小于 30,000,000 字节检查；本次文档阶段不执行构建/发布。
