# tool_fcplug — AGENTS.md

## 项目概览

`tool_fcplug` 是 FlowCloudAI 的 `.fcplug` 插件 CLI（`cargo-fcplug`），提供模板初始化、构建与更新校验能力。  
它是客户端与官方插件仓库之间的统一入口，改动会直接影响插件的兼容性验证链路。

## 构建 / 运行 / 测试 / lint

```bash
cd tool_fcplug
cargo build --release
cargo install --path .
cargo run -- init
cargo run -- build
cargo run -- update
```

## 代码风格与命名约定

- Rust 2024，命令与参数命名清晰、可复现，强调 CLI 可读性。  
- 配置与 manifest 字段保持大小写一致，避免路径拼写漂移。  
- 输出日志兼容脚本解析，适配自动化流水线。  

## 目录结构与模块职责

```text
tool_fcplug/
└── src/
    └── templates/      # 插件初始化模板
```

## 安全 / 禁止事项

- 模板与文档中不得包含真实 API Key、签名密钥或生产端点。  
- 不提交未经校验的外部二进制或不可信下载产物。  
- 改动 manifest 映射需同步检查 loader 和 runtime 兼容性。  

## 提交与 PR 规范

- 提交信息默认中文，单次变更聚焦单一命令链路（init/build/update）。  
- PR 说明需包含执行样例、失败场景和回退策略。  
- 影响协议或兼容性行为时补充迁移说明。  

## 项目特有坑点

- 目标平台缺少 WIT/Wasm 工具链会导致构建失败。  
- manifest 名称、路径和大小写改变会直接导致插件加载失败。  

## 文档同步依据（本次核对）

- 同步时间：2026-06-03 21:04:46 +08:00
- 依据文件：`tool_fcplug/Cargo.toml`、`tool_fcplug/src`
