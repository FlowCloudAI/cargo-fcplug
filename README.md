# cargo-fcplug

`cargo-fcplug` 是 [FlowCloudAI](https://www.flowcloudai.cn) 的插件开发 CLI 工具，提供 `cargo fcplug` 子命令，用于快速创建、迁移、构建和打包 WASM 插件（`.fcplug`）。

---

## 安装

```bash
cargo install --path .
```

安装后，即可使用 `cargo fcplug` 命令。

## 快速开始

```bash
# 创建插件脚手架
cargo fcplug init

# 进入新插件目录后构建 .fcplug 包
cd <plugin-id>
cargo fcplug build
```

构建产物会写入当前插件项目的 `dist/{plugin-id}.fcplug`。

---

## 用法

### 1. 初始化插件项目

```bash
cargo fcplug init
```

交互式创建一个新的 `.fcplug` 插件项目脚手架，包含：

- `manifest.json` — 插件元数据
- `Cargo.toml` / `src/lib.rs` — Rust 插件源码
- `wit/plugin.wit` — WIT 接口定义
- `icon.png` — 默认插件图标
- `README.md` — 项目说明

支持可选参数：

```bash
cargo fcplug init --parent-dir ./plugins
```

### 2. 构建并打包插件

```bash
cargo fcplug build
```

执行流程：

1. 校验 `manifest.json` 格式与字段
2. 编译 WASM（`wasm32-wasip2`，release 模式；各插件 README 中的构建命令已统一为此目标）
3. 定位编译产物
4. 使用 `wasm-tools strip` 优化 WASM 体积
5. 检查 `icon.png` 尺寸与格式
6. 打包为 `dist/{plugin-id}.fcplug`

可选参数：

| 参数 | 说明 |
|------|------|
| `--no-build` | 跳过编译，直接打包已有 WASM |
| `--no-opt`   | 跳过 `wasm-tools strip` 优化 |

### 3. 迁移旧 manifest

```bash
cargo fcplug update
```

将旧 `abi-version = 2` manifest 迁移为 Agreement v1，包括：

- `abi-version` → `agreement-version`
- `kind/llm` / `kind/image` / `kind/tts` → `llm` / `image` / `tts`
- `models: ["a"]` → `models: [{ "id": "a" }]`
- `supports-*` → `default-supports`
- `max-tokens` → 模型级 `max-output-tokens`

`build` 和 `update` 都需要在插件项目根目录执行，因为它们读取当前目录下的 `manifest.json`、`Cargo.toml`、`icon.png` 和编译产物。

---

## 插件包格式

`.fcplug` 本质是一个 ZIP 文件，内部包含：

| 文件 | 说明 |
|------|------|
| `manifest.json` | 插件元数据（`meta` 结构：id、name、version、kind、agreement-version、url 等） |
| `plugin.wasm`   | 编译好的 WASM 组件（构建目标：`wasm32-wasip2`） |
| `icon.png`      | 插件图标（≤ 128×128，正方形） |

当前协议版本：Agreement v1（`agreement-version = 1`）

支持的 `kind`：

- `llm`
- `image`
- `tts`

---

## WIT 接口

插件必须实现以下 WIT 接口：

```wit
package mapper:plugin;

interface mapper {
    map-request: func(input: string) -> string;
    map-response: func(input: string) -> string;
    map-stream-line: func(line: string) -> string;
}

world api {
    export mapper;
}
```

- `map-request`：将 FlowCloudAI 统一请求 JSON 映射为供应商特定格式
- `map-response`：将供应商响应 JSON 映射回 FlowCloudAI 统一格式
- `map-stream-line`：处理 SSE 流式响应的每一行

---

## 技术栈

- **语言**：Rust（Edition 2024）
- **CLI 框架**：[`clap`](https://docs.rs/clap)（derive features）
- **包格式**：ZIP（[`zip`](https://docs.rs/zip) crate）
- **WASM 优化**：[`wasm-tools`](https://github.com/bytecodealliance/wasm-tools) `strip`

---

## 目录结构

```text
tool_fcplug/
├── src/
│   ├── main.rs          # CLI、manifest 校验、构建和打包流程
│   └── templates/       # init 子命令使用的默认模板
├── Cargo.toml           # crate 配置
├── AGENTS.md            # AI 编码助手维护指南
└── README.md            # 当前文档
```

---

## 贡献方式

提交前请运行 `cargo build`。涉及插件包输出或 manifest 迁移时，用一个测试插件目录手动执行 `cargo fcplug build` 或 `cargo fcplug update` 验证。

---

## 许可证

MIT
