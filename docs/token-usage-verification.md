# Token 用量实施与验收

OpenSpec 变更：`add-token-usage-tracking`。候选版本：0.4.0。实施日期：2026-10-17。这里只记录已执行的验证，不代表已创建 GitHub Release。

## 实现

共用 LLM 层按实际 HTTP 尝试观察 usage；Job、连接测试和分析状态各自持有当前操作快照。统计使用独立 SQLite worker、事务终结、日汇总、generation 清空与短期幂等收据。界面提供本次用量、分类汇总、未知覆盖、离线说明及独立清空。

复查修复了一个并发交互问题：读取尚未返回时清空失败，旧读取被作废却仍保留 loading 状态，导致按钮不能再次使用；现在作废旧读取时同时释放其 loading 状态，并有专项回归测试。

## 自动检查

- Linux：82 项 Rust 常规测试通过；默认隔离的 GNOME gsettings 与 X11 剪贴板测试各 1 项另行通过。
- 前端：61 项测试通过，TypeScript 检查与 Vite 生产构建通过。
- 同步服务 12 项、同步协议 6 项测试通过，协议与服务端无业务变更。
- 原生桌面：使用最终候选二进制、隔离 Xvfb/D-Bus/Keyring、本地模拟模型和自托管同步服务执行。包含真实自动分析调度、键盘清空、窄窗与明暗主题截图、实际进程重启。最终运行记录与包检查结果见本节后续补充。
- DEB 验包器包含合法包、依赖缺失/版本不足、入口/桌面文件/安装脚本异常和 30,000,000 字节边界等 12 项正反例。
- Windows 原生 CI、安装/升级验收与各产物 SHA-256/字节数待对应运行完成后补齐。

## 规格与证据对应

| 需求 | 代码与验证入口 |
|---|---|
| Validated provider usage | `usage/types.rs` 的完整/明确零/部分/非法/溢出字段解析测试；前端未知展示测试 |
| Non-additive detail breakdown | 100 输入＋30 输出＝130，缓存 20 与推理 10 不重复相加；别名冲突与子集校验 |
| All model request purposes | 原生翻译、未保存模型的连接测试、手动与真实定时自动分析；四用途并发存储测试 |
| Actual attempts and exactly-once accounting | 共用传输兼容重试 HTTP 计数；重复终结/重复快照、设置归属与发送前校验测试 |
| Usage survives result failures | HTTP 错误、截断、拒绝、非法模型 JSON、超时、超大响应、读取中断；pending 重启恢复 |
| Local cache produces no new consumption | 原生缓存/强制刷新请求次数；Job 缓存来源与零新增展示，缓存不携带 usage |
| Persistent scoped aggregates | SQLite 重开、短期收据清理、分类/分页、密钥无关归类、原生冷启动累计保留 |
| Stable local date attribution | 注入时钟覆盖月界、午夜、夏令时、时区变化与时钟回拨，不修改全局 TZ |
| Independent confirmed reset | 旧 generation 迟到/重复结果、新请求、取消与确认；真实键盘清空保留草稿及术语 |
| Private local statistics | 数据库及同步载荷的内容/凭据标记检查，清空术语不清统计；无原文/译文/提示词/Key 写入接口 |
| Non-blocking statistics failures | 不可用路径、队列满、只读写失败、损坏/未来 schema；内存快照保留，拒绝破坏性重建 |
| Accessible usage presentation and explanation | 主窗/浮窗失败快照、连接测试失败、设置覆盖/筛选/清空组件测试；原生明暗/窄窗/键盘及离线说明 |
| Preserve lightweight offline statistics | 本地查询不请求模型、说明内嵌，无 tokenizer/模型/余额请求；支持包体小于 30,000,000 字节检查 |

## 验证环境与限度

所有模型请求使用 localhost 模拟服务及虚构 Key；没有调用付费模型或账户余额接口。Linux 实测环境为 Ubuntu 26.04，DEB 依赖以最终 ELF 验包结果为准，不据此宣称 Ubuntu 22.04 二进制兼容。

测试修正了隔离夹具：D-Bus 激活服务使用与客户端一致的临时 XDG 路径，清理前卸载其私有 document-portal FUSE 挂载；冷启动后切换到可见浮窗执行 WebDriver 检查，避免隐藏 WebKit 页面挂起。夹具调整没有改变应用凭据策略或放宽断言。

真实服务商 usage 完整性及账单仍由服务商决定；没有联网账单对账承诺。既有真实 GNOME Wayland 按键、Windows 跨应用划词和多显示器人工验收要求继续保留，见既有快捷键变更和手动验收文档；它们不由本次模拟模型测试替代。
