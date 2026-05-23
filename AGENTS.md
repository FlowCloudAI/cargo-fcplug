# tool_fcplug — AGENTS.md

> 本文档面向 AI 编码助手。修改 CLI 前先确认是否影响插件协议、模板输出或现有 `.fcplug` 包兼容性。

## 项目概览

`tool_fcplug` 是 FlowCloudAI 的 Rust CLI 工具，crate / 二进制名为 `cargo-fcplug`，安装后提供 `cargo fcplug` 子命令。它负责创建插件脚手架、校验并打包 WASM 插件，以及把旧 manifest 迁移到当前 Agreement v1 协议。

## 构建 / 运行 / 测试 / lint

```bash
cd tool_fcplug

# 构建 CLI
cargo build
cargo build --release

# 本地安装为 cargo 子命令
cargo install --path .

# 直接运行源码中的 init 子命令
cargo run -- init

# 在测试插件项目根目录验证 build / update
cargo fcplug build
cargo fcplug update
```

当前没有自动化测试或独立 lint 脚本。修改 `build` 流程时，应在一个测试插件目录中运行 `cargo fcplug build`；修改 `update` 时，应准备旧 `abi-version = 2` manifest 做迁移验证。若要调试未安装的源码，从测试插件目录使用 `cargo run --manifest-path <tool_fcplug/Cargo.toml> -- build` 或 `-- update`。

## 代码风格与命名约定

- Rust 使用 Edition 2024，标准命名：类型 `PascalCase`，函数 / 变量 / 模块 `snake_case`，常量 `SCREAMING_SNAKE_CASE`。
- 注释、文档和模板说明使用中文。
- 错误处理使用 `anyhow::Result`，用户可读错误要说明原因和修复提示。
- 终端输出沿用 `[INFO]`、`[WARN]`、`[ERROR]` 彩色前缀，不新增随意格式。
- CLI 参数由 `clap` derive 定义，新增参数需同步 README 和本文件。

## 目录结构与模块职责

```text
tool_fcplug/
├── src/
│   ├── main.rs          # CLI 参数、manifest 校验、构建、优化、打包、迁移
│   └── templates/
│       ├── icon.png     # 默认插件图标
│       ├── plugin.wit   # 默认 WIT 接口
│       └── Readme.md    # init 使用的 README 模板源文件
├── Cargo.toml
├── Cargo.lock
└── README.md
```

关键事实：

- `src/templates/Readme.md` 的文件名大小写要保持不变，`include_bytes!` 在大小写敏感系统上会严格匹配。
- `init` 生成的插件项目文件名是 `README.md`，模板源文件名是 `Readme.md`。
- `AGREEMENT_VERSION = 1` 定义在 `src/main.rs`，修改会影响 App、核心库和插件包。
- `build` 默认执行 `cargo build --target wasm32-wasip2 --release`，再尝试 `wasm-tools strip -a`。
- `icon.png` 缺失时会自动生成默认图标；打包产物仍固定包含 `manifest.json`、`plugin.wasm`、`icon.png`。
- manifest 校验边界在 `src/main.rs` 中集中实现：插件 ID 仅允许小写字母 / 数字 / 连字符，版本必须是三段式 semver，公网 URL 必须使用 HTTPS。

## CLI 子命令

```bash
# 创建插件脚手架
cargo fcplug init
cargo fcplug init --parent-dir ./plugins

# 构建并打包当前插件项目
cargo fcplug build
cargo fcplug build --no-build
cargo fcplug build --no-opt

# 迁移旧 manifest
cargo fcplug update
```

## 插件协议与包格式

`.fcplug` 是 ZIP 包，内部包含：

- `manifest.json`：`meta` 嵌套结构，字段包括 `id`、`name`、`author`、`description`、`version`、`kind`、`agreement-version`、`url`。
- `plugin.wasm`：`wasm32-wasip2` release 产物。
- `icon.png`：不超过 128×128 的正方形 PNG。

插件必须实现 `wit/plugin.wit` 中的三个函数：

- `map-request`
- `map-response`
- `map-stream-line`

旧版 `plugin-info` 已移除，插件信息完全来自 `manifest.json`。

## 提交信息与 PR 规范

- 提交信息默认使用中文，格式建议为“动词 + 范围 + 目的”，例如 `修正 manifest 迁移校验`。
- 一个提交只包含一个明确任务，不混入构建产物、测试插件包或无关格式化。
- PR 说明需写明影响的子命令、是否改变 manifest / WIT 协议、手动验证使用的插件目录和命令输出摘要。
- 协议变更必须同步检查 `core_ai_client`、`app_main` 和 `plugins/*/wit/plugin.wit`。

## 安全 / 禁止事项

- 不提交真实 API Key、供应商私有 URL、测试插件产物或 `target/`。
- 不放宽公网 URL 校验；当前策略允许 HTTPS，HTTP 仅限 localhost / loopback。
- 不删除 manifest 协议版本检查，不把旧协议静默当作新协议处理。
- 不在模板中写入真实作者、密钥或生产端点。

## 项目特有坑点

- `cargo fcplug build` 必须在插件项目根目录运行，因为它读取当前目录的 `manifest.json`、`Cargo.toml` 和 `icon.png`。
- crate 名中的连字符会被替换为下划线来定位 WASM 产物。
- `wasm-tools` 缺失不会使构建失败，只会跳过 strip 并输出警告。
- `update` 只迁移 manifest，不会重写插件 Rust 源码或 WIT 文件。
- 修改模板后需用 `cargo run -- init` 生成新项目，检查生成文件名和内容。
