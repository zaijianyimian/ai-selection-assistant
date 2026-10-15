# AI Selection Assistant

开源 AI 划词翻译桌面客户端。选中文字，按 **Ctrl+Shift+Q**，在独立窗口阅读译文与专业术语解释。支持 Windows 11 和 Ubuntu Linux；使用自己的 OpenAI-compatible 模型服务，无需登录，不依赖本项目运营的服务器。

**技术栈：Tauri 2 + Rust + Vue 3 + TypeScript + Vite + Pinia + SQLite。** SQLite 随应用编译，无需安装数据库服务。没有 Electron、Java 或 Python 应用依赖。MIT 许可。

## 功能

- 全局快捷键可配置；Windows 模拟复制选区、X11 优先 PRIMARY、Wayland 明确回退到手动复制。
- 输入文字、翻译剪贴板、独立置顶翻译窗口、托盘菜单、关闭隐藏与显式退出。
- 一次成功的模型回复返回译文、专业术语翻译、中文解释及可选示例。
- 识别 Java、JVM、数据库、分布式系统、AI / LLM、Agent、机器学习与软件工程术语；提示模型跳过普通词汇。
- 模型设置持久化、测试连接、目标语言、术语开关、深色 / 浅色 / 跟随系统。
- 可选本地术语记忆、熟悉程度反馈、手动 / 应用内每日分析、解释风格与背景偏好；不存原文。
- 密钥仅保存于 Windows Credential Manager 或 Linux Secret Service；不回传已存密钥、不写入配置或日志。
- 输入上限 16,000 字符；响应上限 1 MiB；超时、权限、空选区、快捷键冲突、HTTP 错误均有可恢复提示。

## Windows 11 开发与构建

先安装：

1. Node.js **22.12+ 或 24 LTS**，以及 npm。
2. [Rust / rustup](https://rustup.rs/)，使用 MSVC 工具链，Rust **1.90+**（建议当前 stable）。
3. [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)，选择 **Desktop development with C++**，包含 MSVC x64/x86 工具和 Windows 10/11 SDK。VS Code 自身不包含这些工具。
4. Microsoft Edge WebView2 Runtime（Windows 11 通常已提供）。

在新的 PowerShell 终端中执行：

```powershell
cd D:\mine\helper
rustup default stable-msvc
npm ci
npm run build
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm test
npm run tauri dev
```

路径可替换为自己的项目目录。发布 NSIS 安装包：

```powershell
npm run tauri build -- --bundles nsis
```

也可执行可重复打包脚本，将安装包、直接运行版、许可和 SHA-256 校验文件收集到 `releases/<version>/windows-x64/`：

```powershell
.\scripts\build-windows.ps1
```

脚本会检查每一步的退出状态，不会在构建失败时生成假的安装包；不上传文件。Windows 安装包默认仅为当前用户安装，提供简体中文和英文。直接运行版需要系统已有 WebView2 Runtime，安装包会检查并按需安装该运行库。

安装包在 `src-tauri/target/release/bundle/nsis/`。若出现 `link.exe not found`，安装上面的 C++ 工具和 SDK，关闭终端后重开；必要时使用 **Developer PowerShell for VS**。不要通过更换成 GNU 工具链绕过 Tauri 的 Windows MSVC 要求。

### 缺少 Windows 编译工具时的 WSL 打包方式

Tauri 也提供 Linux → Windows MSVC 的 NSIS 交叉编译方案。需要在 WSL 内安装 Linux 版 Node.js、rustup 和以下工具，不需要将代码上传到 CI：

```bash
sudo apt install clang lld llvm nsis
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
cd /path/to/helper
npm ci
bash scripts/build-windows-wsl.sh
```

输出在 `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/`；设置了 `CARGO_TARGET_DIR` 时以该目录为准。交叉编译生成的程序仍须在 Windows 上验收。配置了静态 C 运行库链接，不要求用户安装 Visual Studio。参考 [Tauri 官方 Windows 打包说明](https://v2.tauri.app/distribute/windows-installer/)。

交叉编译脚本配合 `tauri.windows-cross.conf.json` 使用：由 `cargo-xwin` 和 `+crt-static` 统一静态链接运行库，关闭 Tauri 额外的 VC 静态 / UCRT 动态链接辅助，避免两组链接参数相互排除。此覆盖仅用于交叉编译，原生 Windows 构建仍使用 Tauri 默认配置。

## Ubuntu 开发与构建

适用于 Ubuntu 22.04 / 24.04 及具有 WebKitGTK 4.1 的后续桌面版本。需要完整桌面会话；纯 SSH / 无图形环境不能显示应用。

```bash
sudo apt update
sudo apt install -y build-essential pkg-config curl wget file \
  libwebkit2gtk-4.1-dev libxdo-dev libssl-dev \
  libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev \
  gnome-keyring dbus-user-session wl-clipboard fonts-noto-cjk fonts-noto-color-emoji
```

使用 [rustup](https://rustup.rs/) 安装 Rust stable（1.90+），并安装 Node.js 22.12+ / 24 LTS 和 npm；确保 `node`、`npm`、`cargo` 在 PATH 中。

```bash
cd /path/to/helper
npm ci
npm run build
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm test
npm run tauri dev
```

构建安装包：

```bash
npm run tauri build -- --bundles deb
# 或 AppImage（可能还需发行版提供的 FUSE 运行库）
npm run tauri build -- --bundles appimage
```

输出位于 `src-tauri/target/release/bundle/`。同一份 `node_modules` 不宜跨 Windows / WSL 共用；正常开发应在各自系统的独立工作目录执行 `npm ci`。在 WSL 中建议把项目放在 Linux 文件系统以改善编译性能；显示窗口需要 WSLg。

### X11 和 Wayland

- **X11**：选择文字后触发快捷键，先读 PRIMARY；为空时回退剪贴板，并显示来源提示。
- **Wayland**：先 **Ctrl+C** 复制，再按已可用的快捷键，或点击应用 / 托盘中的 **翻译剪贴板**。应用使用 `wl-paste` 读取、`wl-copy` 复制译文。
- 全局快捷键取决于合成器及 XWayland 支持。若无法注册，可在系统设置里配置自定义快捷键，执行已安装的程序：

```bash
/absolute/path/to/ai-selection-assistant --translate-clipboard
```

应用通过 single-instance 插件把第二次启动转交给正在运行的实例。仍需先手动复制文字。GNOME 如未显示托盘，可安装 AppIndicator 扩展；即使没有托盘，再次启动程序也可重新打开主窗口。

### 系统安全凭据

Ubuntu 需在登录的用户 D-Bus 会话中运行，Secret Service 应已启动且密钥环已解锁。不要用 `sudo npm run tauri dev`。密钥环不可用时会显示错误，应用不会退回明文保存。

### 中文字体与方框文字

界面内置本地 Noto Sans SC Variable 字体（SIL Open Font License 1.1），不依赖在线 Google Fonts。即使精简 Linux 环境缺少中文字库，WebView 内容也能显示中文。原生窗口标题和托盘菜单使用系统字体，因此 Ubuntu 仍建议安装 `fonts-noto-cjk`；安装后完全退出应用再启动。中文显示为 `□□□` 通常是字形缺失，不要对正确的 UTF-8 源文件执行转码。

## 首次使用

1. 进入 **偏好设置**，填写 Base URL、Model 和 API Key。无需认证的本机服务可不填 Key。
2. **测试连接**会使用当前表单发送一条短文本翻译请求，可能消耗少量服务商额度，不保存设置。临时密钥在测试后会从输入框清空；需要保存新 Key 时请重新输入。
3. 点击 **保存设置**，让配置立即生效。API Key 留空会保留现有密钥；删除需勾选 **保存时删除现有 API Key**。
4. 在浏览器、IDE 或 PDF 阅读器中选中文字，按 Ctrl+Shift+Q。在 Wayland 或「仅剪贴板」模式下，先 Ctrl+C。
5. 或直接在主窗口输入文字，点击 **开始翻译** / Ctrl+Enter。
6. 点击复制图标复制译文。关闭窗口会隐藏应用；通过托盘或重新启动打开，选择 **退出应用**才退出。

### Base URL 规则

| 输入示例 | 实际请求地址 |
|---|---|
| `https://api.example.com` | `https://api.example.com/v1/chat/completions` |
| `https://api.example.com/v1/` | `https://api.example.com/v1/chat/completions` |
| `https://api.example.com/api/v3` | `https://api.example.com/api/v3/chat/completions` |
| `https://api.example.com/v1/chat/completions` | 保持不变 |
| `http://localhost:11434/v1` | `http://localhost:11434/v1/chat/completions` |

仅允许 HTTPS，localhost / 127.0.0.1 / ::1 的本机服务可使用 HTTP。URL 不得包含用户名、密码、查询参数或片段。HTTP 重定向不会被跟随，请填写最终 API 地址。

### 结构化输出兼容

- **自动**：DeepSeek 官方 Chat Completions 接口直接使用 `json_object`；其他接口优先 JSON Schema。服务以 400 / 404 / 415 / 422 明确拒绝输出格式时，按 Schema → JSON 模式 → 提示词 JSON 逐级兼容，也识别「只支持 text / json_object」这类枚举错误。
- **JSON 模式**：直接使用 `response_format: { type: "json_object" }`。
- **提示词 JSON**：不发送 `response_format`，依靠提示词约定返回 JSON，适合传统接口，避免已知不支持 Schema 的服务每次发生探测请求。

每个成功回复同时包含翻译和术语，不为每个术语分别调用模型。仅当服务明确拒绝 Temperature 或要求默认值时，省略该参数重试一次；不自动更换模型。一次操作最多 4 个 HTTP 请求，总超时覆盖所有尝试。鉴权失败、限流、普通参数错误不重试。错误提示包含 HTTP 状态及可识别的错误字段 / 错误代码，不直接回显服务原始消息，避免密钥或原文被服务端回显。

DeepSeek 配置示例：Base URL `https://api.deepseek.com/v1`，Model `deepseek-flash`，输出模式选择「自动」或「JSON 模式」。0.1.0 版用户可先选「JSON 模式」绕过其 Schema 兼容缺陷。参考 [DeepSeek Chat Completions 参数](https://api-docs.deepseek.com/api/create-chat-completion/) 和 [JSON 输出说明](https://api-docs.deepseek.com/guides/json_mode/)。

### 术语记忆与个性化

1. 在 **偏好设置 → 记忆与个性化** 开启「记住专业术语」，保存。默认关闭，旧配置不会自动开启。
2. 每次成功翻译后，保存专业术语名称、译名、领域、语言、独立词义及出现时间 / 来源；同一任务的重试幂等，不同翻译分别计数。同名不同领域或不同词义分开保存。不保存原文、完整译文、完整解释或示例。
3. 在 **术语记忆** 搜索、分页浏览，标记「正在学习 / 已熟悉」，或在翻译卡片反馈。出现次数不会自动推断掌握程度。连接测试不计入记忆。
4. 点击 **分析新增记忆**，将最多 100 个术语的摘要、计数和反馈发给当前配置的模型。每批覆盖最多 500 条新增出现 / 反馈记录；剩余记录可继续点击分析。结果仅包含近期领域主题和可查看的证据术语 ID。失败保留旧画像；保留最近 10 个成功版本，可切换。
5. 开启 **个性化解释** 并保存：后续翻译使用明确设置的解释风格、背景，以及最多 8 个与当前文字匹配的术语、近期主题（记忆数据总长最多 2000 字符）。当前原文词义优先，明确反馈优先于主题推断。不需要等待分析即可使用风格和反馈。
6. **自动分析** 单独开启，需记忆已开启：应用运行且无翻译任务时每分钟检查；至少 20 条新增出现 / 反馈记录，自动尝试之间至少 24 小时。尝试时间持久化，失败或重启不会立即重复请求。关闭应用后不运行，不创建系统定时任务。模型请求可能产生 API 费用。
7. 关闭记忆后停止积累，已存数据仍保留。删除单词 / 清空全部需在应用中确认，同时使所有分析版本失效。更改记忆或自动分析开关也会使画像失效；进行中的旧翻译和分析不能写回。需要时重新分析现有术语。

独立词义由模型生成：不支持新增字段的传统模型仍可翻译，但缺少 `senseSummary` 的术语不会存入记忆，会显示警告。同义词定义措辞变化可能形成多条记录；MVP 不做额外合并请求。记忆库在本机以 SQLite 明文保存，可随时删除或停用。

## 隐私与当前限制

- Rust 负责系统操作、安全存储与 HTTP。前端不会取得已存 API Key；密码框只持有用户新输入的替换值，操作结束清空，不写浏览器存储。
- 非敏感配置位于 Tauri app config 目录的 `settings.json`：Windows 通常为 `%APPDATA%\io.github.ai-selection-assistant\`，Linux 为 `~/.config/io.github.ai-selection-assistant/`。
- 凭据项 service 为 `io.github.ai-selection-assistant`，account 为 `provider-api-key`。可在设置里删除；卸载前删除最方便。
- 主动翻译 / 测试会向选定服务发送文本；开启个性化时还发送有限相关术语与主题，开启自动分析后也会按上述规则发送术语摘要。原文和完整译文不落盘，不记录文本日志或埋点；服务商如何保留请求由其政策决定。
- 可选术语记忆位于 Tauri app data 目录的 `memory.sqlite3`：Windows 通常为 `%APPDATA%\io.github.ai-selection-assistant\`，Linux 为 `~/.local/share/io.github.ai-selection-assistant/`（以系统 XDG 配置为准）。其中没有 API Key；清空会删除术语、出现、反馈与画像内容，保留内部递增 ID 和调度时间以防旧请求及重试。
- Windows 自动模式可以恢复原有**文本**，不能完整恢复 HTML、图片等多格式剪贴板。纯图片 / 文件剪贴板会提示先手动复制。要避免任何模拟复制与恢复操作，使用 **仅剪贴板**模式。
- 自动模式不会在模拟复制未产生新剪贴板事件时翻译旧内容。权限较高的目标应用、受保护 PDF、禁用复制的网页可能拒绝读取，应手动复制或输入。
- 术语准确性与模型相关，提示词约束范围与普通词过滤，但没有独立事实检索。鼠标附近定位、流式输出、历史记录和自动更新未加入本 MVP。

## 目录

```text
src/
  components/       Vue 翻译、设置、术语卡片与记忆页面
  stores/           Pinia 状态、原生事件同步
  native.ts         窄范围 Tauri invoke 封装
  types.ts          前端类型（不包含持久化密钥）
src-tauri/src/
  selection.rs      Windows / X11 / Wayland 捕获与复制
  shortcut.rs       快捷键验证与注册
  llm.rs            HTTP、地址规范化、错误处理与兼容
  translation.rs    提示词、JSON Schema、解析与输入验证
  settings.rs       配置文件与 keyring
  memory.rs         SQLite 工作线程、术语、反馈、画像与隐私 epoch
  analysis.rs       分析协议、手动命令与应用内每日调度
  window.rs         窗口和托盘
  lib.rs            Tauri 命令、任务状态、模块协调
openspec/changes/foundation-mvp/
  proposal.md       需求分析
  design.md         技术决策和回退策略
  specs/            能力与验收场景
  tasks.md          实施及验收进度
```

OpenSpec 已作为开发依赖安装并初始化，可执行 `npx openspec --version`、`npm run spec:validate`。无需全局安装。`npm run dev` 仅启动界面预览；浏览器不提供原生功能，也不会伪装成可用的模型客户端。

术语记忆变更的方案、规格与实施进度见 `openspec/changes/personalized-term-memory/`；基础变更保持独立，未自动归档。

## 测试与手动验收

本次环境的实际检查结果及未验证范围见 [验证记录](docs/verification.md)，逐项操作见 [手动验收清单](docs/manual-acceptance.md)。

```bash
npm test
npm run build
npm run spec:validate
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

Rust HTTP 测试使用临时本机 TCP 服务和虚构凭据，不需要真实 API Key，不访问真实付费模型。前端测试涵盖事件顺序、任务恢复、重复请求、错误状态、输出转义与临时密钥生命周期。

构建依据：[Tauri 2 原生依赖](https://v2.tauri.app/start/prerequisites/)、[全局快捷键](https://v2.tauri.app/plugin/global-shortcut/)、[系统托盘](https://v2.tauri.app/learn/system-tray/)。
