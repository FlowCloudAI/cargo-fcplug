# FlowCloudAI 插件 CLI（tool_fcplug）

`tool_fcplug` 提供 FlowCloudAI 的 `.fcplug` 命令行工具链，用于初始化、构建与更新插件清单。  
其目标是让插件项目从创建到构建验证形成统一、可复现的流程。

## 项目简介

仓库以 CLI 能力集中管理模板与打包步骤，配合 `plugins` 仓库中的示例插件进行端到端验证。  
上手前先确认本机已具备相应 Rust/wasm 编译链。

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

1. 执行 `cargo run -- init` 生成模板。  
2. 在 `plugins` 子仓库执行 `cargo fcplug build` 做完整产物生成。  
3. 需要时执行 `cargo run -- update` 验证兼容性与 manifest。  

## 主要功能 / 使用方式

- 插件模板初始化。  
- 插件构建与产物检查。  
- `update` 命令用于兼容性与协议一致性校验。  

## 技术栈

- Rust 2024、clap、WASM、`wit-bindgen`

## 目录结构（仅顶层）

```text
tool_fcplug/
└── src/
    └── templates/
```

## 许可证与贡献方式

- 许可证：仓库未发现独立 `LICENSE` 文件（TODO：与发布仓库确认许可来源）。  
- PR 建议补充 `cargo run -- init/build/update` 的执行结果与失败回放日志。  
- 兼容性改动需写清 API 参数变更与迁移影响。  

文档同步时间：2026-06-03 21:04:46 +08:00
