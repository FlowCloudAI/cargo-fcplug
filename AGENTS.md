# tool_fcplug — AGENTS.md

## 项目概览

`tool_fcplug` 是 FlowCloudAI 的 `.fcplug` CLI（`cargo-fcplug`）实现，覆盖插件模板初始化、构建与兼容性检查。  
该工具负责连接桌面端与官方 `plugins` 仓库的插件运行链路。

## 构建 / 运行 / 测试 / lint

```bash
cd tool_fcplug
cargo build --release
cargo install --path .
cargo run -- init
cargo run -- build
cargo run -- update
```

该仓库未提供独立 lint 脚本，变更应优先确认 `cargo fcplug build` 联动链路。

## 代码风格与命名约定

- Rust 2024，命令参数与配置模型语义明确。  
- CLI 接口和错误信息应稳定可解析，便于脚本与 CI 复用。  
- 文件路径与 manifest 字段必须保持一致，尤其注意大小写敏感平台。  

## 目录结构与模块职责

```text
tool_fcplug/
├── src/          # CLI 核心与命令实现
│   └── templates/ # 插件模板与骨架
└── Cargo.toml    # CLI 依赖与发行配置
```

## 安全 / 禁止事项

- 模板与文档不得包含真实 API Key、签名密钥或生产端点。  
- 不提交未经校验的外部二进制产物。  
- 修改 manifest 映射需同步插件运行时和加载器兼容验证。  

## 提交与 PR 规范

- 提交信息默认中文，单次变更聚焦 `init/build/update` 命令链路。  
- PR 需附 `cargo run -- init/build/update` 结果与异常场景说明。  
- 兼容性改动需补充迁移与联调说明。  

## 项目特有坑点

- 缺少 WIT/Wasm 工具链会导致构建或更新失败。  
- manifest 名称、路径与大小写变更会影响客户端加载。  

文档同步时间：2026-06-08 13:20:10 +08:00
