# {PLUGIN_NAME}

这是由 `cargo-fcplug` 生成的 FlowCloudAI WASM 插件项目。

---

## 插件信息

| 属性 | 值 |
|------|-----|
| ID | `{PLUGIN_ID}` |
| 名称 | `{PLUGIN_NAME}` |
| 类型 | `{PLUGIN_KIND}` |
| 版本 | `{PLUGIN_VERSION}` |

---

## 构建

```bash
# 编译 WASM
cargo build --target wasm32-wasip1 --release

# 打包为 .fcplug
cargo fcplug build
```

构建产物位于 `dist/{PLUGIN_ID}.fcplug`。

---

## WIT 接口

本插件实现了以下 WIT 接口：

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

- `map-request`：将 FlowCloudAI 统一请求 JSON 映射为供应商特定格式。
- `map-response`：将供应商响应 JSON 映射回 FlowCloudAI 统一格式。
- `map-stream-line`：处理 SSE 流式响应的每一行。

---

## manifest.json

```json
{
  "meta": {
    "id": "{PLUGIN_ID}",
    "name": "{PLUGIN_NAME}",
    "version": "{PLUGIN_VERSION}",
    "author": "{PLUGIN_AUTHOR}",
    "description": "{PLUGIN_DESCRIPTION}",
    "kind": "{PLUGIN_KIND}",
    "abi-version": 2
  },
  "models": []
}
```

---

## 许可证

MIT
