# cargo-fcplug

`cargo-fcplug` 是 **FlowCloudAI 插件开发工具**。

它用于帮助开发者快速创建、构建和打包 **FlowCloudAI WASM 插件**。

插件用于为 FlowCloudAI 提供 **AI API 适配能力**，例如：

- LLM（文本生成）
- Image（文生图 / 图生图）
- TTS（语音生成）

插件只负责 **请求和响应的 JSON 映射**，所有网络请求和 API Key 管理由 **Core 引擎**统一处理。

---

# 特性

- 一键生成插件项目模板
- 自动构建 WASM 插件
- 自动优化 WASM（使用 `wasm-opt`）
- 自动打包 `.fcplug` 插件包
- 简单稳定的 **WASM Component 插件接口**

---

# 插件架构

FlowCloudAI 插件采用 **WASM Component Model**。

插件不直接访问网络，也无法读取 API Key。

架构如下：

```
App
│
▼
FlowCloudAI Core
│
├── HTTP Client
├── API Key 管理
├── 重试 / 限流
│
▼
WASM Mapper Plugin
│
▼
Provider API
```
插件只负责：
```
Unified JSON  <->  Provider JSON
```

这样设计的优点：

- 插件安全
- API Key 不会泄露
- 插件实现简单
- 支持任意语言编写插件

---

# 插件包格式

`.fcplug` 是一个 zip 文件，包含：

```
plugin.fcplug
├── manifest.json
├── plugin.wasm
└── icon.png (可选)
````

---

# manifest.json

插件描述文件：

```json
{
  "id": "example-plugin",
  "name": "Example Plugin",
  "version": "0.1.0",
  "author": "Author",
  "description": "Example plugin",
  "kind": "kind/llm",
  "abi_version": 1
}
````

字段说明：

| 字段          | 说明        |
|-------------|-----------|
| id          | 插件唯一 ID   |
| name        | 插件名称      |
| version     | 插件版本      |
| author      | 作者        |
| description | 插件描述      |
| kind        | 插件类型      |
| abi_version | 插件 ABI 版本 |

支持的插件类型：

```
kind/llm
kind/image
kind/tts
```

---

# 安装

安装开发工具：

```bash
cargo install cargo-fcplug
```

---

# 创建插件

创建插件项目：

```bash
cargo fcplug init
```

交互示例：

```
Plugin id [my-plugin]:
Plugin kind (llm|image|tts) [llm]:
Author [unknown]:
Description [example plugin]:
```

生成项目结构：

```
fcplug-my-plugin
 ├── Cargo.toml
 ├── manifest.json
 ├── icon.png
 ├── wit
 │   └── plugin.wit
 └── src
     ├── lib.rs
     └── types.rs
```

---

# 构建插件

进入插件目录：

```
cargo fcplug build
```

构建流程：

1. 校验 `manifest.json`
2. 编译 WASM
3. 使用 `wasm-opt` 优化（如果存在）
4. 打包 `.fcplug`

输出：

```
dist/plugin.fcplug
```

---

# 可选参数

跳过编译：

```
cargo fcplug build --no-build
```

跳过 wasm-opt：

```
cargo fcplug build --no-opt
```

---

# 插件接口

插件使用 **WIT 接口**定义。

```
wit/plugin.wit
```

```wit
package mapper:plugin;

interface mapper {
  plugin-info: func() -> string;
  map-request: func(input: string) -> string;
  map-response: func(input: string) -> string;
}

world api {
  export mapper;
}
```

插件必须实现三个函数：

| 函数           | 说明     |
|--------------|--------|
| plugin-info  | 返回插件信息 |
| map-request  | 映射请求   |
| map-response | 映射响应   |

---

# 示例插件

```rust
struct MyPlugin;

impl Guest for MyPlugin {

    fn plugin_info() -> String {
        let info = PluginInfo {
            id: "example".to_string(),
            version: "0.1.0".to_string(),
            author: "author".to_string(),
            abi_version: 1,
            name: "Example".to_string(),
            description: "Example plugin".to_string(),
            kind: PluginKind::LLM,
        };

        serde_json::to_string(&info).unwrap()
    }

    fn map_request(input: String) -> String {
        input
    }

    fn map_response(input: String) -> String {
        input
    }
}
```

---

# 插件图标

插件可以包含：

```
icon.png
```

要求：

* 最大 **128×128**
* 必须 **正方形**

---

# WASM 构建目标

插件必须编译为：

```
wasm32-wasip2
```

示例：

```
cargo build --target wasm32-wasip2 --release
```

---

# 许可证
MIT