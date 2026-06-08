# FlowCloudAI 插件 CLI（tool_fcplug）

## 项目简介

`tool_fcplug` 提供 `.fcplug` 命令行工具链，覆盖插件模板初始化、构建与兼容性检查。  
它将插件开发流程统一为可复现链路，并对接 `plugins` 示例仓库。

## 快速开始

### 安装与执行

```bash
cd tool_fcplug
cargo build --release
cargo install --path .
cargo run -- init
cargo run -- build
cargo run -- update
```

### 最小示例

1. 执行 `cargo run -- init` 生成新模板。  
2. 安装 `cargo-fcplug` 后切到 `plugins`，执行 `cargo fcplug build`。  
3. 必要时执行 `cargo run -- update` 做兼容检查。  

## 主要功能 / 使用方式

- 插件模板初始化与脚手架生成。  
- 插件构建与产物一致性校验。  
- `update` 命令用于协议兼容与 manifest 校验。  

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
- 兼容性改动需说明参数变化与迁移影响。  

文档同步时间：2026-06-08 13:20:10 +08:00
