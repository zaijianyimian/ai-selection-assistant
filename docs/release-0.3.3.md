# v0.3.3 — 翻译缓存、界面调整与 Windows 安装包（预发布）

发布日期：2026-10-17。提供 Linux amd64 DEB、Windows x64 安装版 EXE 和便携版 ZIP。真实 GNOME Wayland 按键、Windows 跨应用划词与多屏操作仍待人工验收，继续标记为预发布。

## 更新

- 翻译优先查询本次运行的内存缓存；同一原文、服务、模型、语言、参数及个性化上下文匹配时直接复用，不再次请求模型。
- 缓存最多 100 条、序列化内容合计最多 8 MiB，自结果生成起有效 30 分钟；退出应用后清空，不写入磁盘或同步服务。失败结果不缓存，连接测试始终真实请求。
- 显示「来自缓存 · 未请求模型」，新增「重新请求模型」与「清空缓存」。重新请求可能产生 API 费用；缓存命中不重复增加术语出现记录，保存设置会清空缓存。
- 调整术语记忆页的字号、禁用按钮、状态间距与空状态提示，统一浅色和深色样式。
- 修复 Windows 首次启用多端同步时，备份文件以只读句柄刷盘导致迁移失败的问题，保留备份一致性和恢复校验。
- 新 Windows 包包含现有本地术语记忆、关键词/模糊检索、自托管同步、紧凑浮窗和应用图标更新。

## 下载和安装

Linux：下载 `ai-selection-assistant_0.3.3_amd64.deb`，退出旧版本后执行：

```bash
sudo apt install ./ai-selection-assistant_0.3.3_amd64.deb
```

DEB 基于 Ubuntu 26.04 构建，要求 libc6 >= 2.39 及声明的 WebKitGTK/GTK 等依赖，不适用于 Ubuntu 22.04 的预编译安装。DEB 为 13,614,808 字节，严格小于 30,000,000 字节。

Windows：使用 `ai-selection-assistant_0.3.3_x64-setup.exe`，或解压 `ai-selection-assistant_0.3.3_windows-x64-portable.zip` 后运行 `AI-Selection-Assistant.exe`。需要系统 WebView2 Runtime；安装器会检查并按需下载。安装版为 9,206,897 字节，便携 ZIP 为 10,687,445 字节，两者均小于 30,000,000 字节。EXE 未进行代码签名。

Ubuntu GNOME Wayland：在「偏好设置 → 翻译偏好」保存组合键并启用系统快捷键；先 Ctrl+C，再按快捷键。配置读回不代表已通过真实按键验收。

自托管同步服务本次无协议或服务端变更，可继续使用 [v0.3.2 的服务镜像、源码和部署说明](https://github.com/zaijianyimian/ai-selection-assistant/releases/tag/v0.3.2)。模型 API Key、原文和完整译文不参与同步。

## 验证

- 本地前端 46 项、Linux Rust 70 项通过；两项默认跳过的桌面集成测试分别在隔离 D-Bus/dconf 与 Xvfb 中通过。
- 最终 Linux 发布二进制通过 60 项原生端到端检查：真实 Tauri IPC、缓存请求计数、强制刷新、失败不缓存、退出后清空、恢复门控、剪贴板、快捷键、窗口和同步。
- 服务端 12 项、协议 6 项通过；6 个 OpenSpec 变更严格校验通过。
- DEB 验包器 12 项正反例通过；Ubuntu 26.04 隔离容器升级 0.3.2 → 0.3.3，设置、术语 fixture、快捷键启用信息及 dconf 绑定的文件哈希保持不变。
- Windows runner 上 58 项 Rust 测试及 46 项前端测试通过；安装版/便携版原生构建、两次主窗口启动、静默安装与卸载检查通过。记录见附件 `windows-package-verification.json` 及 [GitHub Actions](https://github.com/zaijianyimian/ai-selection-assistant/actions/runs/39014322014)。

所有自动翻译和同步测试使用临时数据、虚构凭据及本地模拟服务；未调用付费模型。自动化启动检查不替代真实桌面操作验收。附件 `SHA256SUMS` 和 `manifest.json` 提供校验值及构建提交。
