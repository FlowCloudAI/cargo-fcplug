# cargo-fcplug — AGENTS.md

> 本文件面向 AI 编程助手。如果你对该子项目一无所知，请先阅读本文档再动手修改代码。

---

## 一、项目概览

`cargo-fcplug` 是 **FlowCloudAI 插件开发 CLI 工具**，提供 `cargo fcplug` 子命令，用于：

- `init` — 交互式创建新的 `.fcplug` 插件项目脚手架。
- `build` — 校验 manifest、编译 WASM、优化、打包为 `.fcplug`。

---

## 二、技术栈

- **语言**：Rust（Edition 2024）
- **CLI 框架**：`clap`（derive features）
- **包格式**：ZIP（`zip` crate）
- **WASM 优化**：`wasm-tools strip`（非 `wasm-opt`）

---

## 三、目录结构

```
cargo-fcplug/
├── src/
│   ├── main.rs            # CLI 入口（init / build 子命令）
│   └── templates/
│       ├── icon.png       # 默认插件图标
│       ├── plugin.wit     # 默认 WIT 接口定义
│       └── Readme.md      # 默认插件 README 模板（R 大写）
├── Cargo.toml
└── AGENTS.md
```

> **注意**：模板文件名为 `Readme.md`（首字母大写 R），`main.rs` 中的 `include_bytes!` 引用必须与文件实际大小写一致，否则在 Linux/macOS 上会编译失败。

---

## 四、CLI 命令

```bash
# 在当前目录下创建新插件脚手架
cargo fcplug init

# 构建并打包当前目录的插件为 .fcplug
cargo fcplug build

# 跳过编译（仅打包已有 wasm）
cargo fcplug build --no-build

# 跳过 wasm 优化
cargo fcplug build --no-opt
```

---

## 五、插件包格式

`.fcplug` 本质是一个 ZIP 文件，内部必须包含：

- `manifest.json` — 元数据（`meta` 嵌套结构：id、kind、version、abi-version、url 等）
- `plugin.wasm` — 编译好的 WASM 组件（target: `wasm32-wasip1`）
- `icon.png` — 可选图标

当前 ABI 版本：`ABI_VERSION = 2`（定义在 `src/main.rs`）。

### manifest.json 结构（实际使用）

```json
{
  "meta": {
    "id": "example-plugin",
    "name": "Example Plugin",
    "version": "0.1.0",
    "author": "Author",
    "description": "Example plugin",
    "kind": "kind/llm",
    "abi-version": 2,
    "url": "https://api.example.com/v1/chat"
  },
  "models": [
    { "id": "model-1", "name": "Model 1" }
  ]
}
```

支持的 `kind`：
- `kind/llm`
- `kind/image`
- `kind/tts`

---

## 六、WIT 接口定义

`wit/plugin.wit` 定义了插件必须实现的三个函数：

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

> **注意**：旧版接口中的 `plugin-info` 函数已移除，插件信息现在完全由 `manifest.json` 提供。

---

## 七、构建目标

插件必须编译为：

```bash
cargo build --target wasm32-wasip1 --release
```

---

## 八、代码风格与约定

- 使用 Rust Edition 2024。
- 错误处理以 `anyhow` 为主。
- 所有用户-facing 的输出使用彩色前缀（`[INFO]`、`[WARN]`、`[ERROR]`）。
- 注释和文档使用中文。

---

## 九、修改代码前的检查清单

- [ ] 是否修改了 `ABI_VERSION`？如果是，请同步检查所有现有插件的 `abi-version` 兼容性。
- [ ] 是否修改了 WIT 接口？请同步更新 `src/templates/plugin.wit` 和所有现有插件的 `wit/plugin.wit`。
- [ ] 是否修改了 manifest 校验逻辑？请确保与现有插件的 `manifest.json` 兼容。
- [ ] 运行 `cargo build` 和 `cargo run -- init` / `cargo run -- build` 测试新功能。
