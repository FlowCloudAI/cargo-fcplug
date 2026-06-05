# FlowCloudAI 插件 CLI（tool_fcplug）

`tool_fcplug` 提供 FlowCloudAI 的 `.fcplug` 命令行工具链，用于初始化、构建与更新插件清单。  
目标是把插件开发从创建、构建到兼容性检查变成可复现流程。

## 项目简介

仓库聚焦 CLI 能力与模板管理，配合 `plugins` 示例仓库完成端到端兼容验证。  
使用前请先确认本机已具备 Rust 与 wasm 工具链。

## 快速开始

### 安装与执行

```bash
cd tool_fcplug
cargo install --path .
cargo run -- init
cargo run -- build
cargo run -- update
```

### 最小示例

1. 执行 `cargo run -- init` 生成新模板。  
2. 切换到 `plugins` 仓库，执行 `cargo fcplug build`。  
3. 需要时执行 `cargo run -- update` 进行兼容检查。  

## 主要功能 / 使用方式

- 插件模板初始化。  
- 插件构建与产物一致性检查。  
- `update` 命令进行协议兼容与 manifest 校验。  

## 技术栈

- Rust 2024、clap、WIT、WASM  

## 目录结构（仅顶层）

```text
tool_fcplug/
└── src/
    └── templates/
```

## 许可证与贡献方式

- 许可证：本仓库未发现独立 `LICENSE`，按仓库当前授权策略执行。  
- PR 建议补充 `cargo run -- init/build/update` 结果与失败回放。  
- 兼容性改动需说明参数变更与迁移影响。  

文档同步时间：2026-06-05 12:44:21 +08:00
