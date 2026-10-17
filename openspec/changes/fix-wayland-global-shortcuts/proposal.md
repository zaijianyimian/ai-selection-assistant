# Proposal

## Why

Ubuntu GNOME Wayland 下，现有 Linux 快捷键依赖只走 X11，插件初始化成功也会被错误地当作快捷键可用，用户可能安装成功却无法唤起翻译。需要把已有的剪贴板命令入口接入持久的系统快捷键，并在重启、升级和保存失败时保持可解释、可恢复的状态。

## What Changes

- 按桌面会话选择后端：Windows/X11 保留原生注册；GNOME Wayland 增加用户主动启用的系统自定义快捷键；其他 Wayland 桌面提供手动配置说明。
- 系统绑定使用固定命令 `/usr/bin/ai-selection-assistant --translate-clipboard` 和稳定的应用专属配置路径，复用已有单实例转发，不包含进程号、临时 D-Bus 地址或常驻辅助服务。
- 首次运行提供启用入口；后续启动只检查，不重复创建、自动修复或弹出配置对话框。提供状态刷新、显式重新应用和停用入口，尊重用户在系统设置中的修改。
- 区分快捷键后端能力、已保存的期望组合键、系统实际绑定和错误状态；GNOME 的“已配置”不冒充端到端触发已验证。
- 让启用、改键和停用具备可恢复的提交与补偿，保留其他应用的配置，并复用现有设置恢复约束。
- 保留 Wayland 先复制再翻译的行为、窗口位置限制、快捷键仅本机保存及 DEB 小于 30,000,000 字节的要求。
- 本变更不实现 Portal 客户端、自动模拟复制、GNOME 扩展、自动近选区定位，也不修复微信配置；Portal 留作跨桌面扩展方向。

## Capabilities

### New Capabilities

- `desktop-shortcuts`：按平台接入桌面快捷键，管理 GNOME 持久绑定、状态展示、生命周期及失败恢复。

### Modified Capabilities

无。当前 `openspec list --specs --json` 返回空列表，相关历史契约仍位于已完成但未归档的变更中。本次新增独立的快捷键能力，不修改这些变更；实现必须同时保持 `foundation-mvp` 的选区/剪贴板约定、`fix-settings-result-and-deb-reliability` 的补偿约束、`add-local-term-search-and-nearby-popup` 的 Wayland 窗口限制和包体积要求，以及同步变更中的本机快捷键边界。

## Impact

- Rust：`src-tauri/src/shortcut.rs`、`lib.rs`、`settings_commit.rs` 及其测试；增加本机桌面绑定状态、GNOME 适配与非秘密恢复信息。
- Vue/TypeScript：设置和翻译入口的后端状态、启用/停用/重新应用操作、准确的 Wayland 文案与组件测试。
- Linux 打包与文档：核对 `gsettings` 运行依赖和安装路径，扩展现有 DEB 校验及 Ubuntu Wayland 人工验收；安装脚本不写用户桌面配置。
- 不增加模型、运行时解释器或后台守护进程；构建时重新核验实际 DEB 字节数。规划阶段不构建、不发版、不修改桌面设置。
