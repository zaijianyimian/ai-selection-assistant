# Personal sync protocol v1

服务根路径下使用 `/v1/`，JSON camelCase，所有 DTO 拒绝未知字段；动作 `type` 使用 snake_case。公共 Rust 类型位于 `crates/sync-protocol`；固定 observe 请求样例位于其 `tests/observe.json`，往返测试随 crate 运行。数据是精简术语，不是 SQLite 文件，也没有模型 API Key、原文、完整译文、画像或搜索索引。

## Connection and authorization

`POST /v1/auth/check` 和 `/v1/devices/enroll` 接收 `{protocol:1,password,deviceName}`。check 只返回 Context，不创建设备；enroll 返回 `{context,deviceId,token}`，token 只在此次响应出现。以后使用 `Authorization: Bearer ...`；密码/令牌不可放 URL。`GET /v1/devices` 和 `DELETE /v1/devices/{id}` 管理个人设备。健康检查 `/healthz` 不返回业务信息。

Context = `{protocol,workspaceId,historyId,memoryGeneration,revision}`。登录凭据与 SSH 无关。客户端校验证书，仅 HTTPS；loopback HTTP 仅作开发测试，不跟随携带认证的重定向。工作区或 history 改变必须暂停确认，不能直接重放旧队列。

## Push and conflicts

`POST /v1/push` 接收 `{protocol,workspaceId,historyId,operations}`。每个 operation 为 `{id,generation,action}`，id 在工作区中永久幂等；同 ID 不同载荷为 400。同一批次可包含两类动作，只有 memory 动作核验 generation。

| action.type | 字段 |
|---|---|
| observe | term={original,translation,domain,sense,language}、event={id,at,source}、incarnation（可 null）、tombstoneRevision |
| feedback | key、incarnation、baseRevision、value=unknown/learning/known |
| delete | key、incarnation |
| preference | field=theme/targetLanguage/explanationStyle、baseRevision、value（现有枚举） |

术语 key 是 `t1:` + SHA-256（UTF-8 紧凑 JSON 数组 `[normalized,domain.trim(),sense.trim(),language.trim()]`）；normalized 为名称按空白拆分、单空格连接、Unicode lowercase。key format v1 的固定样例由 crate 测试锁定。不同领域/词义/语言不合并；译名/原词展示值以首个接受值为准。

旧记录 ID 是 `e1:` + SHA-256（`["occurrence1",jobUUID,termKey]` 的 UTF-8 紧凑 JSON），因此同库克隆不会双计，新的独立翻译仍分别累计。客户端整数 ID 不上云。

接受响应为 `{context,receipts:[{id,revision}]}`，收据不保存正文。冲突为 409 `{code,context,conflicts:[{opId,code,remoteValue,remoteRevision}]}`。失败批次没有部分业务提交。逐字段基线不匹配且值不同需用户选择；值相同幂等满足，不能靠本机时间选胜者。响应丢失先以原 ID/原载荷重试，再处理冲突，不能修改歧义请求的正文。

## Pull, deletion and reset

`GET /v1/changes?stream=memory|preferences&after=0&limit=500`；第一响应固定 until，后续请求增加 `until=...`。Page = `{context,stream,until,nextCursor,hasMore,changes:[{revision,change}]}`。两个 stream 分别保存 cursor；记忆暂停时不拉其正文，不推进其 cursor。`change.type` 为 term/occurrence/feedback/preference/deleted/reset/redacted，具体字段以共享类型为准。

已删除正文从旧 changes 清理为 redacted，占位仍推进 cursor；这不是任意历史正文快照。客户端必须先处理删除/代次，再推送旧操作。单词生命周期用服务器分配的 incarnation 标识，旧生命周期事件永不复活；确认 tombstone 后的 observe 携带其 tombstoneRevision 才可开启新生命周期。

`POST /v1/memory/reset` 为 `{protocol,workspaceId,historyId,id,generation}`，generation 为期望旧代次。成功产生新代次，重试同 id 只返回原收据；旧记忆队列作废，偏好和凭据不删除。恢复备份另行生成新 historyId、撤销令牌，客户端需人工选择恢复基线。

## Bounds and failures

请求/响应实际 JSON 均最多 1 MiB；push 最多 100 操作，pull 最多 500 条。名称/译名/词义/语言分别最多 120/200/120/40 个 Unicode 字符，source 最多 40 字符；不允许控制字符。旧本机数据超限不截断或丢弃，报告不可同步项。字段值只能用已定义枚举。

错误统一为 Failure，不返回内部数据库/解析错误或请求原文。400=无效协议/载荷，401=需登录，409=冲突/代次/历史/配额，413=超限，429=限流（Retry-After），5xx=暂时不可用。未来协议主版本不兼容时暂停，不能猜测解析。

## Service persistence and resource bounds

Service schema v1 is initialized under one SQLite transaction. `meta` contains the workspace UUID, restore-history UUID, memory generation and Argon2id hash. `devices` stores token SHA-256 digests, never raw tokens. `terms`, `events`, `preferences` and `changes` hold accepted state, and durable `receipts` retain only operation ID, payload digest and revision. WAL, FULL synchronous and secure_delete are enabled. The data directory must be private and local to one service (not NFS). Restore requires the service lock, an integrity-checked schema-v1 backup, and rotates history/revokes tokens before publication.

Defaults: 20 active devices; 5 password attempts per immediate-peer IP per 60 seconds; 2 concurrent Argon2 verifications; 60 authenticated requests/device/minute. Behind the supplied Caddy, password IP limiting conservatively aggregates clients behind the proxy rather than trusting spoofable forwarded headers. 429 includes `Retry-After: 60`; the client honors 1–300 seconds. Argon2id v19 uses 65536 KiB memory, 3 passes and parallelism 1; the combined automated HTTP tests execute actual verification with these parameters. Rate limits are process-local; tokens, receipts and accepted data persist across restart.

Default storage quota is 1 GiB of active SQLite pages (`(page_count-freelist_count)*page_size`); configure `SYNC_STORAGE_BYTES`. It includes metadata and receipts. It is not a hard cap on physical DB/WAL/backup size: provision additional disk space and monitor it. Additions exceeding quota roll back atomically. Reading, deletion and device revocation remain available. TLS/authentication/body validation still apply to these actions.
