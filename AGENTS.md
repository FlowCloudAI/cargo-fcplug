# tool_fcplug — AGENTS.md

## 项目概览

`tool_fcplug` 是 FlowCloudAI 的 `.fcplug` 工具链（`cargo-fcplug`），覆盖插件模板初始化、构建与兼容性检查。  
它是桌面端与官方插件仓库之间的统一入口，影响插件加载一致性。

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

- Rust 2024 严格约定，命令参数与配置模型语义清晰。  
- 接口和错误信息应稳定可解析，便于脚本与 CI 复用。  
- 文件路径与 manifest 字段大小写保持一致，避免跨平台差异。  

## 目录结构与模块职责

```text
tool_fcplug/
├── src/
│   └── templates/  # 插件模板与骨架
└── Cargo.toml      # CLI 依赖与发行配置
```

## 安全 / 禁止事项

- 模板与文档禁止包含真实 API Key、签名密钥和生产端点。  
- 不提交未校验的外部二进制产物。  
- 修改 manifest 映射需同步插件运行时和 loader 兼容验证。  

## 提交与 PR 规范

- 提交信息默认中文，单次变更聚焦命令链路（init/build/update）。  
- PR 需附 `cargo run -- init/build/update` 结果与失败场景说明。  
- 兼容性改动应补充迁移说明。  

## 项目特有坑点

- 缺少 WIT/Wasm 工具链会导致构建或更新失败。  
- manifest 名称、路径与大小写变更会影响客户端加载。  

文档同步时间：2026-06-05 12:44:21 +08:00
