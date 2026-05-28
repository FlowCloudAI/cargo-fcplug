# tool_fcplug — AGENTS.md

## 项目概览

`tool_fcplug` 是 FlowCloudAI 插件 CLI（`cargo-fcplug`），用于 `.fcplug` 插件的初始化、构建、校验与更新链路。  
它统一规范 manifest、WASM 产物与能力映射入口，负责为插件仓库提供一致性入口。

## 构建 / 运行 / 测试 / lint

```bash
cd tool_fcplug
cargo build --release
cargo install --path .
cargo run -- init
cargo run -- build
cargo run -- update
```

仓库未提供独立 lint 脚本，代码质量检查以构建成功、命令行为和示例验证为准。

## 代码风格与命名约定

- Rust 2024，命令参数与校验逻辑建议结构清晰、报错信息具可执行建议。  
- CLI 输出保持稳定的关键字段（输入路径、错误类型、产物路径）。  
- 约定文件名、目录名与 manifest 关键词大小写统一。

## 目录结构与职责

```text
tool_fcplug/
├── src/       # 命令入口、参数解析、构建链路
└── templates/ # 插件初始化模板
```

## 安全 / 禁止事项

- 不在模板中写入任何真实 API Key 或服务端密钥。  
- 插件产物不得携带未校验二进制来源。  
- 更新协议版本需同步核对 `core_ai_client` 与 `plugins` 约定。

## 贡献方式与 PR 规范

- 每次变更说明 `init/build/update` 的输入与输出行为。  
- 提交时附最小可复现命令与风险边界。  
- 提交信息默认中文。

## 项目特有坑点

- 构建目标依赖 `wasm32-wasip2` 环境，验证前确保目标已安装。  
- `plugin.wasm` 与 `manifest` 一致性错误常见于配置文件名、路径或版本字段拼写。

## 文档同步依据（本次核对）

- 同步时间：2026-05-28 18:02:58 +08:00  
- 依据文件：`tool_fcplug/Cargo.toml`、`tool_fcplug/src`、`flowcloudai.projects.json`
