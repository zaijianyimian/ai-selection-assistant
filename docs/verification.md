# 验证记录

## DeepSeek 兼容修复 0.1.1（2026-10-15）

- 根据 DeepSeek 官方 Chat Completions / JSON 输出文档，官方域名 `api.deepseek.com` 的自动模式改用 `json_object`。`deepseek-flash` 是有效的官方模型名。本次未取得真实服务的错误响应，因此只能确认旧版存在兼容缺陷，不能仅凭截图断言线上失败的唯一原因。
- 其他服务明确拒绝格式时按 Schema → JSON → 提示词逐级回退，增加枚举错误识别。明确拒绝 Temperature 时仅省略该参数重试一次；不更换模型，最多四次请求，共享原有总超时。
- 错误显示 HTTP 状态和白名单内的字段 / 代码，不显示服务原始错误，避免回显密钥或原文。通用 400、认证失败等不做盲目重试。
- `npm test`：5 个文件、11 项通过；`npm run build`：类型检查和 Vite 构建通过。
- Windows 目标后端测试使用 cargo-xwin 编译，在本机 Windows 实际执行：21 项通过、0 失败、0 ignored。新增覆盖官方域名精确匹配、枚举拒绝三级回退、Temperature 省略、通用错误不重试及脱敏、最大请求次数。HTTP 服务为本机临时服务，凭据均为虚构值。
- 0.1.1 Windows x64 发布构建退出码为 0，NSIS 安装包（8,850,232 字节）和直接运行版（19,939,328 字节）位于 `releases/0.1.1/windows-x64/`，附许可、使用说明和 SHA-256 校验值。PE 导入检查通过，直接依赖均为系统 DLL。Rust 格式检查、WSL 构建脚本语法检查通过。
- 打包期间用户的 0.1.0 实例仍在运行，因此未终止该实例、未启动会激活旧实例的 0.1.1 做窗口验证。本版未验证实际安装 / 卸载及真实模型服务。
- 尚未用真实 DeepSeek API Key 测试；手动步骤：托盘退出旧实例 → 启动 0.1.1 → 配置 `https://api.deepseek.com/v1`、`deepseek-flash`、输出「自动」 → 测试连接 → 保存设置 → 翻译 `The JVM uses generational garbage collection.` → 检查译文和术语卡片。不要把 API Key 发给维护者。

## Windows exe 发布构建（2026-10-15）

- 使用 Tauri CLI 2.12.3、Rust 1.90.0 的 `x86_64-pc-windows-msvc` 目标、cargo-xwin 0.23.1、LLVM 21 和 NSIS 在 Ubuntu WSL 交叉编译；代码未上传到远程服务。
- 已生成 Windows x64 NSIS 安装包和直接运行版。安装范围为当前用户，支持简体中文 / 英文，最终产物收集于 `releases/0.1.0/windows-x64/`，带许可和 SHA-256 校验文件。
- 首次链接发现 Tauri 的静态 VC / 动态 UCRT 辅助与 cargo-xwin 的全静态 CRT 配置冲突，已通过专用 `tauri.windows-cross.conf.json` 修正；没有删除 SQLite 或其他核心功能。
- PE 导入检查：应用为 COFF x86-64，版本 0.1.0，直接依赖均为 Windows 系统 DLL，没有额外的 VC Runtime 或 WebView2Loader DLL 依赖。WebView2 Runtime 仍由系统提供 / 安装包按需安装。
- 生产 exe 在本机 Windows 实际启动并持续运行 10 秒，未退出，标准错误为空；测试后只终止了此次启动的测试进程。此项不代表已经目视验收全部窗口。
- Windows 原生测试可执行文件在本机运行：16 项通过、0 失败、0 ignored，覆盖设置、协议错误与兼容回退、记忆持久化与分析、个性化请求、快捷键语法和解析。测试凭据为虚构值，HTTP 服务为本机临时服务。
- 前端 11 项测试和生产构建通过。PowerShell 打包脚本及 WSL 脚本语法检查通过。
- 安装包未签名。尚未执行实际安装 / 卸载、跨软件划词、真实 API 和真实凭据持久化的手动验收；步骤见 [验收清单](manual-acceptance.md)。原生 Windows 开发工具链仍未安装，WSL 交叉编译成功不能等同于本机 `cargo check` 可用。

## personalized-term-memory 本次变更（2026-10-15）

OpenSpec proposal / design / 三项能力规格 / tasks 位于 `openspec/changes/personalized-term-memory/`。本次未使用真实 API Key 或付费模型，也没有上传代码。

| 检查 | 实际结果 |
|---|---|
| `npm run build` | 通过 Vue / TypeScript 检查与 Vite 构建 |
| `npm test` | 5 个文件，11 项通过；包含独立开关、反馈及删除确认 |
| `npm run spec:validate` | 两个变更的严格校验通过 |
| `cargo check --locked` | Ubuntu 26.04 WSL / Rust 1.93.1 通过 |
| 隔离 Xvfb Rust 测试 | 17 项全部通过，0 ignored，包含原有 X11 捕获测试 |
| `cargo fmt -- --check` | 通过 |
| `npm run tauri dev -- --no-watch` | 已重新编译并启动更新后的 Ubuntu WSLg 原生应用 |
| Windows `cargo check --locked` | 实际执行，失败于 MSVC `link.exe` 缺失 |
| Windows Tauri dev | 使用已运行的前端服务实际尝试，进入 Cargo 构建后同样被缺失的 `link.exe` 阻塞 |
| 浏览器界面检查 | 已打开术语记忆页面，中文可读，暂停状态、分析条件、空列表和分页显示正常；此项不验证原生命令 |

新增 Rust 验证覆盖：SQLite 重新打开后的持久化；同任务幂等和新任务重复计数；同词多义；明确反馈；删除使旧写入及画像提交失效；无原文 / 解释 / 示例 / Key 列；分析快照后新增记录保留；非法证据不替换旧画像；100 个术语的有界批次；自动尝试时间跨重启保留；单词边界匹配和 2000 字符上下文限制；旧配置默认关闭记忆。

本地 HTTP 组合测试实际执行了翻译提取（展示关闭时仍检测）→ SQLite 保存 → 反馈 → 结构化分析兼容回退 → 画像持久化 → 后续个性化翻译请求，检查分析请求不含原文标记或完整解释，并检查明确风格 / 背景 / 熟悉程度进入提示词。此测试调用生产模块，使用临时 TCP 服务与虚构凭据；它不等同于真实原生窗口全流程或真实模型语义验收。

仍未验证：Windows 原生编译 / 启动及跨软件划词；真实系统凭据的持久化；真实模型的解释效果；自动调度持续运行 24 小时、休眠恢复的完整桌面行为。日间边界和重启抑制已有自动测试，操作步骤见 [手动验收](manual-acceptance.md)。WSLg 仍有 Mesa/EGL 和 GTK scale-factor 环境警告，未伪装成完整图形硬件验收。

## 基础 MVP 的先前验证

日期：2026-10-15。工作目录起初为空。本次没有使用真实 API Key、调用付费模型、创建远程仓库或上传代码。

## 实际执行结果

| 检查 | 结果与范围 |
|---|---|
| OpenSpec 安装 / 初始化 | 项目内安装 1.14.2，建立 foundation-mvp 的 proposal / design / 四项能力 spec / tasks |
| `npm run spec:validate` | `openspec validate --all --strict` 通过 |
| `npm run build` | 通过，包含 Vue / TypeScript 检查与 Vite 生产构建 |
| `npm test` | 4 个文件、8 项测试通过 |
| `cargo check --locked` | Ubuntu 26.04 WSL，Rust 1.93.1：通过 |
| Rust 测试 | 10 项通过，包含下面独立 Xvfb 环境中的 X11 捕获与复制测试 |
| `cargo fmt -- --check` | 通过 |
| `npm run tauri dev -- --no-watch` | Ubuntu WSLg：编译、启动原生应用；用户截图确认窗口及设置界面出现 |
| Windows `cargo check` / Tauri dev | 实际尝试，失败于环境缺少 MSVC `link.exe`，不能声称 Windows 原生编译已通过 |
| `npm audit --omit=dev` | 0 漏洞 |
| 中文字体修复 | 初始 WSL 的 `fc-list :lang=zh-cn` 为空，截图出现方框；源文件 UTF-8 有效，无 U+FFFD。已打包本地 Noto Sans SC 字体并安装系统 CJK 字体，`fc-match` 现返回 Noto Sans CJK SC |
| 字体打包 | 98 个外置 WOFF2 子集，约 4.49 MB，其他小子集由构建内联；CSP 显式允许同源与内联字体；OFL 许可包含在 dist/licenses/ 中 |

开发环境的 Node 为 24.21.0，Tauri CLI 为 2.12.3，Tauri Rust 为 2.12.4。TypeScript 7 与 vue-tsc 的实际兼容错误已通过固定 TypeScript 5.9 修复，没有跳过类型检查。

## 测试覆盖

- 前端：原生事件比快照更新时的顺序、重复提交、IPC 失败恢复、窗口载入时恢复任务、模型输出 HTML 转义、错误/加载状态、密钥输入清空。
- 字体：以严格 UTF-8 解码源码，检查中文界面及原生错误信息的字符均落在已打包字体的覆盖范围。
- 设置：持久化往返、覆盖保存、无密钥字段序列化、非法限值、URL 中凭据和密钥换行拒绝。
- 模型协议：临时 HTTP 服务验证地址拼接、Schema 请求、明确拒绝后的单次回退、401 脱敏且不重试、提示词模式一次请求。
- 解析：代码围栏 JSON、空值/非法输出、关闭术语时清空、16,000 字符边界。
- 快捷键：默认与自定义组合合法性。
- X11：独立虚拟显示器中设置 PRIMARY 与 CLIPBOARD，调用生产捕获实现，分别取得正确内容；复制中文译文并回读一致。

X11 测试默认被明确标记为需要独立显示器，避免日常 `cargo test` 操作用户剪贴板。本次已显式运行，10 项全部通过，没有跳过：

```bash
sudo apt install xvfb xauth
cd src-tauri
xvfb-run -a env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 \
  AI_SELECTION_TEST_X11=1 cargo test --locked -- --include-ignored
```

仅在隔离的 Xvfb 中使用以上命令。普通 `cargo test` 执行其余 9 项，并报告该隔离测试为 ignored。

## 仍需真实环境手动验收

- Windows 11：安装 MSVC C++ 工具和 SDK 后完成本机 check、test、dev；跨软件模拟 Ctrl+C、权限受限程序、剪贴板恢复和快捷键冲突。
- 完整 Ubuntu GNOME / KDE 桌面：真实 Wayland 复制读取、合成器全局快捷键策略、系统自定义快捷键回退、托盘行为。
- 真实 Windows Credential Manager / Linux Secret Service 的保存、读取、删除和重启后复用。本次 WSL 用户会话没有已解锁的 Secret Service，界面已显示对应错误；未创建明文备用存储或弱化密钥环。
- 真实模型服务的成功请求、模型语义准确性、实际计费与限流。HTTP 测试使用虚构 Key 和本地响应，不能替代服务商验收。
- 发布版 NSIS / deb / AppImage 安装包、其他屏幕 DPI、多显示器以及原生菜单字体效果。

WSLg 启动时观察到 Mesa / EGL 和 GTK scale-factor 环境警告，应用未因此退出；这些日志没有被当作无错误的完整硬件图形验收。详细操作见 [手动验收](manual-acceptance.md)。

完整 npm 开发依赖审计仍报告 OpenSpec 间接依赖 braces 的 4 条 high 关联记录（当前 registry 没有对应修复版）。它们属于开发 CLI 的 glob 解析依赖，不进入桌面生产包；未采用破坏性降级或强行修改上游来隐藏记录。
