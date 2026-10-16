# Design

## Context

参见 proposal.md。原有 Memory::view 使用三个字段的 LIKE 与 ID 倒序；独立词义未参与。既有数据库保留 terms/events/profiles/metadata，SQLite bundled 编译参数已包含 SQLITE_ENABLE_FTS5。Tauri 现成提供 cursor_position、monitor_from_point、work_area、scale_factor、set_position，无需新增桌面扩展。

此前修复已通过 23 项前端、31 项 Rust、16 项原生 WebDriver、6 项验包检查，并完成干净容器实装；本变更基于该本地提交，最后统一发 v0.2.0。

## Goals / Non-Goals

**Goals:** 无模型本地关键词/全文与模糊相关检索；数据删除与索引一致；源应用失焦前取得锚点；紧凑弹窗边缘避让；最终 DEB <30,000,000 字节。

**Non-Goals:** 向量语义理解、模型扩展词、外部文档/翻译历史检索；精确跨应用选区矩形、GNOME 扩展、Wayland 全局坐标与跨重启恢复。用户已接受 X11 自动定位与 Wayland 同次运行保留手动位置。

## Decisions

### 1. 四字段本地检索与受限模糊排序

新增 search.rs，输入为 query（最多 200 个字符）、mode（keyword/fuzzy）与 offset；IPC mode 可省略以兼容旧调用。空查询返回原有最近词条，分页仍为每页 30。

关键词拆成最多 8 个空白分隔片段；对每个片段，在 original/translation/domain/sense 中至少一处 LIKE 命中，各片段之间 AND。SQL 参数化，显式转义百分号、下划线、反斜线，不拼接用户 SQL。结果按名称/译名精确、前缀、包含和词义命中评分，再按 ID 稳定排序。

模糊模式合并三组有界候选：FTS5 trigram 命中、LIKE 命中与最近 500 条词条，去重后进行轻量词形相似度排序。每组最多 500；查询最多使用 8 个关键词和受限的 trigram，最多 1500 个候选。短于 3 个字符的查询不依赖 trigram。优先精确/前缀/包含；对 3–64 个字符的词做有界编辑距离（短词最多 1，较长词最多 2）补充轻微拼写错误。模糊 total 表示有界候选中的匹配数，界面和文档说明此限制，不声称全库语义召回。结果包含原有完整词条，保持反馈/删除功能。

**替代方案：** embeddings 不符合用户的无模型与包体要求；只用三字段 LIKE 无法覆盖词义或轻微错拼。FTS5 已在当前依赖中，索引配合轻量排序不新增运行库。

### 2. 外部内容 FTS 索引及自动迁移

在现有数据库内建立 terms_fts（original/translation/domain/sense，external content=terms、trigram tokenizer），用插入/更新/删除触发器与原表事务同步；启用 FTS secure-delete。只索引已有精简字段。

使用 metadata 的 search_index_version 和表存在性决定首次 rebuild；保留现有 user_version=1 和原表结构，使旧数据 ID/计数/反馈/画像均不迁移。初始化索引在事务中完成；失败按现有记忆存储错误路径报告，不返回伪造结果。旧版本打开同一数据库时不会因 user_version 升高而拒绝，已有 bundled SQLite 也支持 FTS5；不删除其他表来回滚。测试覆盖旧表已有数据、插入/更新/删除/清空、重开和索引只含保留字段。

### 3. 查询与模式的前端状态

Memory store 新增 mode 和搜索中状态；提交新查询或切换模式时 offset=0。保留 revision 防迟到响应覆盖；分页使用已提交查询/模式，避免只编辑输入框就改变翻页结果。MemoryView 增加模式控件、词义搜索占位和「本地词法检索，无模型」说明；错误与空结果继续可恢复。搜索不调用 analysis 或模型，不增加联网路径。

### 4. 弹窗锚点与平台降级

start_job 在 popup 请求且源窗口未失焦时读取锚点，传入异步 run_job；保持先 capture 再 show 的顺序。新窗口定位函数为纯几何计算：使用同一监视器的物理坐标、工作区及窗口尺寸，把 12/16 逻辑像素间距按该监视器 scale 转换；右/下空间不足则左/上翻转，随后夹紧。负原点和大 DPI 单元测试不依赖真实显示器。

window::show_translation 在捕获完成后 show/unminimize/focus，随后使用保留的锚点定位；GTK 首次映射可能暂时返回 0×0 外框尺寸，此时异步让出事件循环、最多等待 500 ms 的 configure 更新再计算，避免误判为不可定位。自动定位失败只产生 placement warning，不能吞掉有效译文。手动「打开悬浮窗」不强制使用旧锚点。

Wayland 分支不查询全局锚点、不调用 set_position；复用同一 WebviewWindow 并沿用隐藏逻辑，使手动位置由合成器在同次运行保留。UI 明确自动贴近限制。默认弹窗改为 440×600（最小 360×380），保留滚动与原有控件。不会为位置保存其他应用内容或伪造 Wayland 坐标。

### 5. 包体门槛与发版

linux_deb.py 在解包前检查实际文件 stat size，>=30,000,000 即失败；build-linux-deb.sh 已调用成品验证，所以同一门槛覆盖构建和发布前检查。负向测试用稀疏文件验证刚好达线/超线必败，不把模型、Python、驱动或测试夹具打入 DEB。最终仅依赖既有原生库。

版本改为 0.2.0，更新 README、手动验收、验证记录。重跑原有回归、搜索与位置测试、真实 X11 原生快捷键弹窗和搜索 IPC、干净容器安装/启动、成品大小与依赖检查，记录 Wayland/Windows 未实测范围。全部完成后再提交推送并创建 GitHub Release，核对上传资产的 SHA-256；之后执行用户关机请求。

## Risks / Trade-offs

- [短中文词不适合 trigram] → LIKE 覆盖短查询，测试中文 1/2/3 字、标点和英文混合。
- [有界模糊候选可能漏掉很老的错拼词] → 记录 1500 上限，精确关键词模式不限制整库匹配，界面不宣传真正语义检索。
- [索引包含已删除信息] → 同事务触发器和 secure-delete，验证删除/清空后搜索及重启都无旧命中。
- [Wayland 不允许绝对定位] → 使用用户已确认的同次运行手动位置方案，不依赖坐标读取成功。
- [DPI 与多屏偏移] → 统一物理坐标，使用监视器工作区并覆盖负原点/放大比例；未知信息回退不阻断翻译。

## Migration Plan

现有数据库首次打开自动加索引，不改业务表、ID 或 user_version；可备份数据库后升级。新增 mode 为可选 IPC 字段，前后端由同一包发布。无模型下载或新的服务配置。发布前保留本地修复检查点，新增代码与规格单独提交，不改写已完成的修复任务。
