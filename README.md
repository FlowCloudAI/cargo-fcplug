# FlowCloudAI 插件 CLI（tool_fcplug）

`tool_fcplug` 提供 `.fcplug` 插件链路命令，覆盖初始化、构建和更新检查。  
它用于统一插件产物结构，确保 manifest、WASM 与协议字段对齐。

## 快速开始

### 安装与执行

```bash
cd tool_fcplug
cargo install --path .
cargo run -- init
cargo run -- build
```

### 最小示例

1. 安装 CLI：`cargo install --path .`。  
2. 在插件工程执行 `cargo fcplug init`（或仓库约定命令）初始化模板。  
3. 执行 `cargo fcplug build`，确认产物成功输出。

## 主要功能 / 使用方式

- `.fcplug` 初始化模板生成。  
- 构建流程封装与错误归因。  
- 更新/迁移命令用于 ABI 与能力协议核对。

## 技术栈

- Rust 2024、CLI 约束层、WASM 打包与 `.fcplug` 协议校验。

## 目录结构（仅顶层）

```text
tool_fcplug/
├── src/
└── templates/
```

## 许可证与贡献方式

许可证以仓库声明为准。  
提交前确保 `cargo run -- update` 与主链路输出可复现，说明兼容与失败场景处理。
