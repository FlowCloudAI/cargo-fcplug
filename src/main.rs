use anyhow::{Result, anyhow};
use clap::Parser;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use url::Url;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// 终端颜色重置码
const COLOR_RESET: &str = "\x1b[0m";
/// 绿色（用于 [INFO]）
const COLOR_GREEN: &str = "\x1b[32m";
/// 黄色（用于 [WARN]）
const COLOR_YELLOW: &str = "\x1b[33m";
/// 红色（用于 [ERROR]）
const COLOR_RED: &str = "\x1b[31m";
/// 青色（用于步骤序号）
const COLOR_CYAN: &str = "\x1b[36m";
/// 暗淡色（用于辅助信息）
const COLOR_DIM: &str = "\x1b[2m";

/// 打印绿色 [INFO] 信息到 stdout
fn info(msg: &str) {
    println!("{COLOR_GREEN}[INFO]{COLOR_RESET} {msg}");
}

/// 打印暗淡色的辅助信息到 stdout
fn detail(msg: &str) {
    println!("{COLOR_DIM}      {msg}{COLOR_RESET}");
}

/// 打印带序号的构建步骤标题
fn step(index: u32, total: u32, msg: &str) {
    println!("\n{COLOR_CYAN}[{}/{}]{COLOR_RESET} {msg}", index, total);
}

/// 打印黄色 [WARN] 警告信息到 stdout
fn warning(msg: &str) {
    println!("{COLOR_YELLOW}[WARN]{COLOR_RESET} {msg}");
}

/// 打印红色 [ERROR] 错误信息到 stderr
fn error(msg: &str) {
    eprintln!("{COLOR_RED}[ERROR]{COLOR_RESET} {msg}");
}

/// 将字节数转换为人类可读的容量字符串（B / KB / MB / GB）
fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    if bytes < KB {
        format!("{} B", bytes)
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    }
}

/// 将 `Instant` 转换为耗时字符串（如 "1.2s" 或 "350ms"）
fn elapsed_str(start: Instant) -> String {
    let d = start.elapsed();
    if d.as_secs() >= 1 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}ms", d.as_millis())
    }
}

/// 校验插件 ID 格式：必须以英文小写字母开头，仅允许小写字母、数字和连字符，
/// 长度不超过 64，且不能以连字符结尾或包含连续连字符。
fn validate_plugin_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(anyhow!("plugin id cannot be empty"));
    }
    if id.len() > 64 {
        return Err(anyhow!(
            "plugin id is too long ({} chars, max 64)",
            id.len()
        ));
    }
    if !id.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(anyhow!(
            "plugin id must start with a lowercase letter, got '{}'",
            id.chars().next().unwrap()
        ));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        let bad: Vec<char> = id
            .chars()
            .filter(|c| !c.is_ascii_lowercase() && !c.is_ascii_digit() && *c != '-')
            .collect();
        return Err(anyhow!(
            "plugin id contains invalid characters: {:?}\n       Only lowercase letters, digits, and hyphens are allowed",
            bad
        ));
    }
    if id.ends_with('-') || id.contains("--") {
        return Err(anyhow!(
            "plugin id cannot end with a hyphen or contain consecutive hyphens"
        ));
    }
    Ok(())
}

/// 校验作者字段：非空，长度不超过 128，仅允许可打印 ASCII 字符。
fn validate_author(author: &str) -> Result<()> {
    if author.is_empty() {
        return Err(anyhow!("author cannot be empty"));
    }
    if author.len() > 128 {
        return Err(anyhow!(
            "author is too long ({} chars, max 128)",
            author.len()
        ));
    }
    if !author.chars().all(|c| c >= ' ' && c <= '~') {
        return Err(anyhow!(
            "author must contain only printable ASCII characters"
        ));
    }
    Ok(())
}

/// 校验描述字段：长度不超过 100 字符。
fn validate_description(description: &str) -> Result<()> {
    if description.len() > 100 {
        return Err(anyhow!(
            "description is too long ({} chars, max 100)",
            description.len()
        ));
    }
    Ok(())
}

/// 校验版本号格式：必须为三段式 semver（如 "1.0.0"）。
fn validate_version(version: &str) -> Result<()> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return Err(anyhow!(
            "version must be in semver format (x.y.z), got \"{}\"",
            version
        ));
    }
    for part in &parts {
        if part.parse::<u32>().is_err() {
            return Err(anyhow!(
                "version component \"{}\" is not a valid number",
                part
            ));
        }
    }
    Ok(())
}

/// 校验 URL 策略：公网必须使用 HTTPS，本地服务允许 HTTP。
fn validate_url(raw: &str) -> Result<()> {
    let parsed = Url::parse(raw).map_err(|e| anyhow!("invalid URL \"{}\": {}", raw, e))?;

    match parsed.scheme() {
        "https" => Ok(()),
        "http" => {
            let host = parsed
                .host_str()
                .ok_or_else(|| anyhow!("HTTP URL must contain a host"))?;
            if host.eq_ignore_ascii_case("localhost") {
                return Ok(());
            }
            if let Ok(ip) = host.parse::<IpAddr>() {
                if ip.is_loopback() {
                    return Ok(());
                }
            }
            Err(anyhow!(
                "HTTP is only allowed for localhost/loopback endpoints; use HTTPS for public endpoints"
            ))
        }
        s => Err(anyhow!("URL scheme must be http or https, got \"{}\"", s)),
    }
}

/// 当前插件协议版本号
const AGREEMENT_VERSION: u32 = 1;
/// 合法的插件类型
const KINDS: [&str; 3] = ["llm", "image", "tts"];
/// 默认插件图标（PNG）
static DEFAULT_ICON: &[u8] = include_bytes!("../src/templates/icon.png");
/// 默认 README 模板
static DEFAULT_README: &[u8] = include_bytes!("../src/templates/Readme.md");
/// 默认 WIT 接口定义模板
static DEFAULT_WIT: &[u8] = include_bytes!("../src/templates/plugin.wit");

/// CLI 顶层参数解析器
#[derive(Parser)]
#[command(
    bin_name = "cargo fcplug",
    version,
    about = "FlowCloudAI plugin development tool",
    long_about = "cargo-fcplug is a helper tool for building and packaging FlowCloudAI WASM plugins.\n\nIt can scaffold a new plugin project or build an existing plugin into a .fcplug package."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// CLI 子命令枚举
#[derive(clap::Subcommand)]
enum Commands {
    Build(BuildArgs),
    Init(InitArgs),
    Update(UpdateArgs),
}

/// `build` 子命令参数
#[derive(Parser)]
#[command(about = "Build and package a plugin into .fcplug")]
struct BuildArgs {
    /// 跳过编译，直接打包已有 WASM
    #[arg(long)]
    no_build: bool,

    /// 跳过 wasm-tools strip 优化
    #[arg(long)]
    no_opt: bool,
}

/// `init` 子命令参数
#[derive(Parser)]
#[command(about = "Create a new plugin project scaffold")]
struct InitArgs {
    /// 指定创建项目的父目录（默认为当前目录）
    #[arg(long)]
    parent_dir: Option<String>,
}

/// `update` 子命令参数
#[derive(Parser)]
#[command(about = "Migrate manifest.json from ABI v2 to agreement v1")]
struct UpdateArgs {}

/// `manifest.json` 中的 `meta` 字段结构
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ManifestMeta {
    id: String,
    name: String,
    author: String,
    description: String,
    version: String,
    kind: String,
    agreement_version: u32,
    url: String,
}

/// `manifest.json` 顶层结构
#[derive(Deserialize)]
struct Manifest {
    meta: ManifestMeta,

    /// 类型特定的扩展字段（如 `models`、`voices` 等）
    #[serde(flatten)]
    ext: Value,
}

/// 交互式询问用户输入，若输入为空则返回默认值
fn ask(prompt: &str, default: &str) -> Result<String> {
    use std::io::{Write, stdin, stdout};

    print!("{} [{}]: ", prompt, default);
    stdout().flush()?;

    let mut buf = String::new();
    stdin().read_line(&mut buf)?;

    let s = buf.trim();
    if s.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(s.to_string())
    }
}

/// 调用 `cargo build` 编译插件为 WASM（target: wasm32-wasip2, release）
fn build_wasm() -> Result<()> {
    let start = Instant::now();

    let status = Command::new("cargo")
        .args(["build", "--target", "wasm32-wasip2", "--release"])
        .status()
        .map_err(|e| anyhow!("failed to invoke cargo: {} (is cargo in PATH?)", e))?;

    if !status.success() {
        let code = status
            .code()
            .map(|c| format!("exit code {}", c))
            .unwrap_or_else(|| "killed by signal".into());
        return Err(anyhow!("cargo build failed ({})", code));
    }

    info(&format!("Build succeeded in {}", elapsed_str(start)));
    Ok(())
}

/// 在 `target/wasm32-wasip2/release` 下查找编译生成的 `.wasm` 文件
fn find_wasm() -> Result<PathBuf> {
    let cargo_toml: Value = toml::from_str(&fs::read_to_string("Cargo.toml")?)?;
    let crate_name = cargo_toml["package"]["name"]
        .as_str()
        .ok_or(anyhow!("missing package.name in Cargo.toml"))?
        .replace('-', "_");

    let expected = Path::new("target/wasm32-wasip2/release").join(format!("{}.wasm", crate_name));

    if expected.exists() {
        let size = fs::metadata(&expected)?.len();
        info(&format!(
            "Found WASM artifact: {} ({})",
            expected.display(),
            human_size(size)
        ));
        Ok(expected)
    } else {
        Err(anyhow!(
            "expected WASM not found at: {}\n       Hint: ensure `crate-type = [\"cdylib\"]` is set in [lib] and the crate name matches",
            expected.display()
        ))
    }
}

/// 使用 `wasm-tools strip` 去除 WASM 中的调试信息，减小体积
fn optimize_wasm(wasm: &Path) -> Result<()> {
    let size_before = fs::metadata(wasm)?.len();
    let start = Instant::now();
    let tmp_path = wasm.with_extension("wasm.opt.tmp");

    let status = Command::new("wasm-tools")
        .args(["strip", "-a"])
        .arg(wasm)
        .arg("-o")
        .arg(&tmp_path)
        .status();

    match status {
        Ok(s) if s.success() => {
            let tmp_size = fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0);
            if tmp_size == 0 {
                let _ = fs::remove_file(&tmp_path);
                warning("wasm-tools produced an empty file, keeping original");
                return Ok(());
            }

            if let Err(e) = fs::rename(&tmp_path, wasm) {
                fs::copy(&tmp_path, wasm).map_err(|ce| {
                    anyhow!("failed to replace wasm (rename: {}, copy: {})", e, ce)
                })?;
                let _ = fs::remove_file(&tmp_path);
            }

            let size_after = fs::metadata(wasm)?.len();
            let saved = size_before.saturating_sub(size_after);
            let pct = if size_before > 0 {
                (saved as f64 / size_before as f64) * 100.0
            } else {
                0.0
            };
            info(&format!(
                "wasm-tools strip done in {} ({} → {}, -{:.1}%)",
                elapsed_str(start),
                human_size(size_before),
                human_size(size_after),
                pct,
            ));
            Ok(())
        }
        Ok(s) => {
            let _ = fs::remove_file(&tmp_path);
            let code = s
                .code()
                .map(|c| format!("exit code {}", c))
                .unwrap_or_else(|| "killed by signal".into());
            warning(&format!("wasm-tools strip failed ({}), skipping", code));
            Ok(())
        }
        Err(e) => {
            warning(&format!(
                "wasm-tools not found ({}), skipping optimization",
                e
            ));
            detail("Install: cargo install wasm-tools");
            Ok(())
        }
    }
}

/// 读取并校验当前目录下的 `manifest.json`，返回解析后的结构体
fn validate_manifest() -> Result<Manifest> {
    let manifest_path = Path::new("manifest.json");

    let mut file = File::open(manifest_path).map_err(|e| {
        anyhow!(
            "cannot open manifest.json: {}\n       Hint: run this command from the plugin project root",
            e
        )
    })?;

    let mut buf = String::new();
    file.read_to_string(&mut buf)?;

    let raw: Value = serde_json::from_str(&buf).map_err(|e| {
        let line = e.line();
        let col = e.column();
        let loc = format!(" at line {}, column {}", line, col);
        anyhow!("invalid manifest.json{}: {}", loc, e)
    })?;

    if raw
        .get("meta")
        .and_then(|m| m.get("abi-version"))
        .is_some()
        && raw
            .get("meta")
            .and_then(|m| m.get("agreement-version"))
            .is_none()
    {
        return Err(anyhow!(
            "manifest.json still uses abi-version; run `cargo fcplug update` to migrate to agreement-version = 1"
        ));
    }

    let manifest: Manifest = serde_json::from_value(raw).map_err(|e| {
        anyhow!(
            "invalid Agreement v1 manifest: {}\n       Hint: run `cargo fcplug update` for old ABI v2 manifests",
            e
        )
    })?;

    let meta = &manifest.meta;

    // 字段校验
    validate_plugin_id(&meta.id).map_err(|e| {
        anyhow!(
            "meta.id is invalid: {}\n       Example: \"id\": \"my-awesome-plugin\"",
            e
        )
    })?;

    if meta.name.trim().is_empty() {
        return Err(anyhow!(
            "meta.name cannot be empty\n       Example: \"name\": \"My Plugin\""
        ));
    }

    if meta.description.trim().is_empty() {
        return Err(anyhow!(
            "meta.description cannot be empty\n       Example: \"description\": \"My plugin\""
        ));
    }
    validate_description(&meta.description).map_err(|e| {
        anyhow!(
            "meta.description is invalid: {}\n       Keep the UI description concise",
            e
        )
    })?;

    validate_version(&meta.version).map_err(|e| {
        anyhow!(
            "meta.version is invalid: {}\n       Example: \"version\": \"0.1.0\"",
            e
        )
    })?;

    validate_author(&meta.author).map_err(|e| {
        anyhow!(
            "meta.author is invalid: {}\n       Example: \"author\": \"yourname\"",
            e
        )
    })?;

    validate_url(&meta.url).map_err(|e| {
        anyhow!(
            "meta.url is invalid: {}\n       Example: \"url\": \"https://api.example.com/v1\"",
            e
        )
    })?;

    let valid_kinds = KINDS;
    if !valid_kinds.contains(&meta.kind.as_str()) {
        return Err(anyhow!(
            "meta.kind is \"{}\", must be one of: {}\n       Example: \"kind\": \"llm\"",
            meta.kind,
            valid_kinds.join(", ")
        ));
    }

    if meta.agreement_version != AGREEMENT_VERSION {
        return Err(anyhow!(
            "meta.agreement-version is {}, expected {}\n       Hint: update to the current agreement version",
            meta.agreement_version,
            AGREEMENT_VERSION
        ));
    }

    // 验证类型特定字段
    validate_ext(&meta.kind, &manifest.ext)?;

    info(&format!(
        "Manifest OK: {} ({}) v{} [{}]",
        meta.name, meta.id, meta.version, meta.kind
    ));
    detail(&format!("Author: {}", meta.author));

    Ok(manifest)
}

/// 验证类型特定的扩展字段
fn validate_ext(kind: &str, ext: &Value) -> Result<()> {
    let model_ids = validate_models(ext)?;
    validate_default_model(ext, &model_ids)?;

    match kind {
        "llm" => validate_llm_ext(ext)?,
        "tts" => {
            // voices 对 TTS 是必需的
            let voices = ext.get("voices").and_then(|v| v.as_array());
            match voices {
                Some(arr) if !arr.is_empty() => {
                    for (i, v) in arr.iter().enumerate() {
                        if v.get("id")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .is_empty()
                        {
                            return Err(anyhow!("voices[{}].id cannot be empty", i));
                        }
                        if v.get("name")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .is_empty()
                        {
                            return Err(anyhow!("voices[{}].name cannot be empty", i));
                        }
                    }
                }
                _ => {
                    return Err(anyhow!(
                        "\"voices\" is required for TTS plugins and cannot be empty\n       ..."
                    ));
                }
            }
            if let Some(default_voice) = ext.get("default-voice").and_then(|v| v.as_str()) {
                let Some(voices_arr) = voices else {
                    return Err(anyhow!("\"voices\" is required for TTS plugins"));
                };
                let found = voices_arr
                    .iter()
                    .any(|v| v.get("id").and_then(|x| x.as_str()) == Some(default_voice));
                if !found {
                    return Err(anyhow!("\"default-voice\" must match a voice id"));
                }
            }
        }
        "image" => {
            // supported-sizes 对 Image 推荐但非必需，仅警告
            if ext.get("supported-sizes").is_none() {
                warning("\"supported-sizes\" not specified for image plugin");
            }
        }
        _ => {}
    }

    Ok(())
}

/// 校验通用 models 数组，返回模型 ID 列表。
fn validate_models(ext: &Value) -> Result<Vec<String>> {
    let models = ext
        .get("models")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("\"models\" is required and must be a non-empty array"))?;

    if models.is_empty() {
        return Err(anyhow!(
            "\"models\" cannot be empty\n       Example: \"models\": [{{\"id\":\"model-name\"}}]"
        ));
    }

    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, model) in models.iter().enumerate() {
        let id = model
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if id.is_empty() {
            return Err(anyhow!("models[{}].id cannot be empty", i));
        }
        if !seen.insert(id.to_string()) {
            return Err(anyhow!("duplicate model id: {}", id));
        }
        ids.push(id.to_string());

        validate_positive_u64(model, "context-window-tokens", &format!("models[{i}]"))?;
        validate_positive_u64(model, "max-output-tokens", &format!("models[{i}]"))?;
    }

    Ok(ids)
}

/// 校验默认模型引用。
fn validate_default_model(ext: &Value, model_ids: &[String]) -> Result<()> {
    if let Some(default_model) = ext.get("default-model").and_then(|v| v.as_str()) {
        if !model_ids.iter().any(|id| id == default_model) {
            return Err(anyhow!("\"default-model\" must match a model id"));
        }
    }
    Ok(())
}

/// 校验 LLM 专属扩展字段。
fn validate_llm_ext(ext: &Value) -> Result<()> {
    validate_thinking_efforts(ext.get("default-thinking-efforts"), "default-thinking-efforts")?;

    if let Some(models) = ext.get("models").and_then(|v| v.as_array()) {
        let default_supports = ext.get("default-supports").unwrap_or(&Value::Null);
        let default_thinking = support_bool(default_supports, "thinking").unwrap_or(false);
        let default_efforts = ext
            .get("default-thinking-efforts")
            .and_then(|v| v.as_array())
            .map(|arr| arr.len())
            .unwrap_or(0);

        for (i, model) in models.iter().enumerate() {
            let label = format!("models[{i}].thinking-efforts");
            validate_thinking_efforts(model.get("thinking-efforts"), &label)?;

            let thinking = model
                .get("supports")
                .and_then(|v| support_bool(v, "thinking"))
                .unwrap_or(default_thinking);
            let efforts_len = model
                .get("thinking-efforts")
                .and_then(|v| v.as_array())
                .map(|arr| arr.len())
                .unwrap_or(default_efforts);
            if !thinking && efforts_len > 0 {
                return Err(anyhow!(
                    "models[{}] has thinking-efforts but supports.thinking is false",
                    i
                ));
            }
        }
    }

    Ok(())
}

/// 读取 supports 中的 bool 字段。
fn support_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(|v| v.as_bool())
}

/// 校验 thinking effort 枚举且不重复。
fn validate_thinking_efforts(value: Option<&Value>, label: &str) -> Result<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let arr = value
        .as_array()
        .ok_or_else(|| anyhow!("{} must be an array", label))?;
    let mut seen = BTreeSet::new();
    for effort in arr {
        let effort = effort
            .as_str()
            .ok_or_else(|| anyhow!("{} must contain strings only", label))?;
        if !matches!(effort, "none" | "minimal" | "low" | "medium" | "high" | "xhigh") {
            return Err(anyhow!("{} contains invalid effort: {}", label, effort));
        }
        if !seen.insert(effort) {
            return Err(anyhow!("{} contains duplicate effort: {}", label, effort));
        }
    }
    Ok(())
}

/// 校验可选正整数。
fn validate_positive_u64(value: &Value, key: &str, label: &str) -> Result<()> {
    if let Some(v) = value.get(key) {
        let n = v
            .as_u64()
            .ok_or_else(|| anyhow!("{}.{} must be a positive integer", label, key))?;
        if n == 0 {
            return Err(anyhow!("{}.{} must be greater than 0", label, key));
        }
    }
    Ok(())
}

/// 检查当前目录下的 `icon.png`，若不存在则生成默认图标；
/// 校验图标为正方形且尺寸不超过 128×128。
fn check_icon() -> Result<PathBuf> {
    let icon = Path::new("icon.png");

    if !icon.exists() {
        info("icon.png not found, generating default icon");
        fs::write(icon, DEFAULT_ICON)?;
    }

    let (w, h) = read_png_size(icon)?;

    if w != h {
        return Err(anyhow!(
            "icon.png must be square, got {}x{}\n       Hint: resize to {}x{}",
            w,
            h,
            w.min(h),
            w.min(h)
        ));
    }

    if w > 128 || h > 128 {
        return Err(anyhow!(
            "icon.png must be ≤ 128x128, got {}x{}\n       Hint: resize the image to 128x128 or smaller",
            w,
            h
        ));
    }

    let size = fs::metadata(icon)?.len();
    info(&format!("Icon OK: {}x{} ({})", w, h, human_size(size)));

    Ok(icon.to_path_buf())
}

/// 将 `manifest.json`、`plugin.wasm` 和 `icon.png` 打包为 `.fcplug`（ZIP 格式）
fn package(wasm: &Path, icon: &Path, plugin_id: &str) -> Result<()> {
    let dist = Path::new("dist");

    let old_pkg = dist.join(format!("{}.fcplug", plugin_id));
    if old_pkg.exists() {
        fs::remove_file(&old_pkg)?;
    }

    fs::create_dir_all(dist)?;

    let fcplug_filename = format!("{}.fcplug", plugin_id);
    let fcplug_path = dist.join(&fcplug_filename);

    let fcplug = File::create(&fcplug_path)?;
    let mut zip = ZipWriter::new(fcplug);

    let opt = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut pack_file = |name: &str, path: &Path| -> Result<()> {
        let size = fs::metadata(path)?.len();
        zip.start_file(name, opt)?;
        std::io::copy(&mut File::open(path)?, &mut zip)?;
        detail(&format!("+ {} ({})", name, human_size(size)));
        Ok(())
    };

    pack_file("manifest.json", Path::new("manifest.json"))?;
    pack_file("plugin.wasm", wasm)?;
    pack_file("icon.png", icon)?;

    zip.finish()?;

    let pkg_size = fs::metadata(&fcplug_path)?.len();
    info(&format!(
        "Packaged: dist/{} ({})",
        fcplug_filename,
        human_size(pkg_size)
    ));

    Ok(())
}

/// 读取 PNG 文件头，解析并返回图像的宽度和高度
fn read_png_size(path: &Path) -> Result<(u32, u32)> {
    let mut f = File::open(path)?;
    let mut buf = [0u8; 24];
    f.read_exact(&mut buf).map_err(|e| {
        anyhow!(
            "failed to read icon.png header: {} (file may be corrupted or too small)",
            e
        )
    })?;

    if &buf[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(anyhow!(
            "icon.png is not a valid PNG file (bad magic bytes)\n       Hint: ensure the file is a real PNG, not a renamed JPEG"
        ));
    }

    let w = u32::from_be_bytes(buf[16..20].try_into()?);
    let h = u32::from_be_bytes(buf[20..24].try_into()?);

    Ok((w, h))
}

/// 执行 `init` 子命令：交互式创建新插件项目脚手架
fn run_init(parent_dir: Option<String>) -> Result<()> {
    println!("== Create new FlowCloudAI plugin ==\n");

    let id = ask("Plugin id", "my-plugin")?;
    validate_plugin_id(&id)?;

    let mut kind: String;
    loop {
        kind = ask("Plugin kind (llm|image|tts)", "llm")?;
        if kind != "llm" && kind != "image" && kind != "tts" {
            error("Invalid plugin kind, choose one of: llm, image, tts");
        } else {
            break;
        }
    }

    let author = ask("Author", "unknown")?;
    validate_author(&author)?;

    let description = ask("Description", "example plugin")?;
    validate_description(&description)?;

    let base = parent_dir.unwrap_or_else(|| ".".into());
    let root = Path::new(&base).join(format!("fcplug-{}", id));

    if root.exists() {
        return Err(anyhow!(
            "directory already exists: {}\n       Hint: choose a different id or remove the existing directory",
            root.display()
        ));
    }

    println!();

    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("wit"))?;

    write_manifest(&root, &id, &kind, &author, &description)?;
    detail("+ manifest.json");

    write_cargo(&root, &id)?;
    detail("+ Cargo.toml");

    write_lib(&root)?;
    detail("+ src/lib.rs");

    write_wit(&root)?;
    detail("+ wit/plugin.wit");

    write_icon(&root)?;
    detail("+ icon.png");

    write_readme(&root)?;
    detail("+ README.md");

    write_gitignore(&root)?;
    detail("+ .gitignore");

    println!();
    info(&format!(
        "Plugin scaffold created at: {}",
        root.canonicalize().unwrap_or(root.clone()).display()
    ));
    detail(&format!("cd {} && cargo fcplug build", root.display()));

    Ok(())
}

/// 执行 `build` 子命令：校验、编译、优化、打包插件
fn run_build(args: BuildArgs) -> Result<()> {
    let total_start = Instant::now();

    let total_steps = 3 + (!args.no_build as u32) + (!args.no_opt as u32);
    let mut current_step: u32 = 0;

    if !Path::new("Cargo.toml").exists() {
        return Err(anyhow!(
            "Cargo.toml not found in current directory: {}\n       Hint: run this command from the plugin project root",
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<unknown>".into())
        ));
    }

    current_step += 1;
    step(current_step, total_steps, "Validating manifest.json");
    let manifest = validate_manifest()?;

    if !args.no_build {
        current_step += 1;
        step(
            current_step,
            total_steps,
            "Building WASM (target: wasm32-wasip2, profile: release)",
        );
        build_wasm()?;
    } else {
        info("Skipping build (--no-build)");
    }

    current_step += 1;
    step(current_step, total_steps, "Locating WASM artifact");
    let wasm = find_wasm()?;

    if !args.no_opt {
        current_step += 1;
        step(current_step, total_steps, "Optimizing WASM binary");
        optimize_wasm(&wasm)?;
    } else {
        info("Skipping optimization (--no-opt)");
    }

    current_step += 1;
    step(current_step, total_steps, "Checking icon & packaging");
    let icon = check_icon()?;

    package(&wasm, &icon, &manifest.meta.id)?;

    println!();
    println!(
        "{COLOR_GREEN}✓ Build complete in {}{COLOR_RESET}",
        elapsed_str(total_start)
    );

    Ok(())
}

/// 执行 `update` 子命令：将旧 ABI v2 manifest 迁移到 Agreement v1。
fn run_update(_args: UpdateArgs) -> Result<()> {
    let manifest_path = Path::new("manifest.json");
    let mut buf = String::new();
    File::open(manifest_path)
        .map_err(|e| anyhow!("cannot open manifest.json: {}", e))?
        .read_to_string(&mut buf)?;

    let mut value: Value = serde_json::from_str(&buf).map_err(|e| {
        anyhow!(
            "invalid manifest.json at line {}, column {}: {}",
            e.line(),
            e.column(),
            e
        )
    })?;

    let changed = migrate_manifest_to_v1(&mut value)?;
    if changed {
        fs::write(
            manifest_path,
            format!("{}\n", serde_json::to_string_pretty(&value)?),
        )?;
        info("manifest.json migrated to agreement-version = 1");
    } else {
        info("manifest.json is already agreement-version = 1");
    }

    validate_manifest()?;
    Ok(())
}

/// 执行确定性的旧协议迁移。
fn migrate_manifest_to_v1(value: &mut Value) -> Result<bool> {
    let root = value
        .as_object_mut()
        .ok_or_else(|| anyhow!("manifest.json root must be an object"))?;
    let meta = root
        .get_mut("meta")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| anyhow!("manifest.json must contain object field \"meta\""))?;

    let mut changed = false;

    if let Some(abi_version) = meta.remove("abi-version") {
        if abi_version.as_u64() != Some(2) {
            return Err(anyhow!(
                "only abi-version = 2 can be migrated automatically, got {}",
                abi_version
            ));
        }
        changed = true;
    }

    if meta.get("agreement-version").and_then(|v| v.as_u64()) != Some(AGREEMENT_VERSION as u64) {
        meta.insert(
            "agreement-version".to_string(),
            serde_json::json!(AGREEMENT_VERSION),
        );
        changed = true;
    }

    if let Some(kind) = meta.get_mut("kind").and_then(|v| v.as_str().map(str::to_string)) {
        let next = match kind.as_str() {
            "kind/llm" => Some("llm"),
            "kind/image" => Some("image"),
            "kind/tts" => Some("tts"),
            _ => None,
        };
        if let Some(next) = next {
            meta.insert("kind".to_string(), Value::String(next.to_string()));
            changed = true;
        }
    }

    changed |= migrate_models_to_objects(root)?;
    changed |= migrate_default_supports(root)?;
    changed |= migrate_max_tokens(root)?;

    Ok(changed)
}

/// 将 `models: ["a"]` 迁移为 `models: [{"id": "a"}]`。
fn migrate_models_to_objects(root: &mut serde_json::Map<String, Value>) -> Result<bool> {
    let Some(models) = root.get_mut("models").and_then(|v| v.as_array_mut()) else {
        return Ok(false);
    };

    let mut changed = false;
    for model in models.iter_mut() {
        if let Some(id) = model.as_str() {
            *model = serde_json::json!({ "id": id });
            changed = true;
        }
    }
    Ok(changed)
}

/// 迁移旧的顶层 supports-* 字段。
fn migrate_default_supports(root: &mut serde_json::Map<String, Value>) -> Result<bool> {
    let mut changed = false;
    let mappings = [
        ("supports-thinking", "thinking"),
        ("supports-tools", "tools"),
        ("supports-stream", "stream"),
    ];

    let mut moved = Vec::new();
    for (old_key, new_key) in mappings {
        if let Some(value) = root.remove(old_key) {
            moved.push((new_key, value));
            changed = true;
        }
    }

    if moved.is_empty() {
        return Ok(changed);
    }

    let default_supports = root
        .entry("default-supports".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let obj = default_supports
        .as_object_mut()
        .ok_or_else(|| anyhow!("default-supports must be an object"))?;

    for (key, value) in moved {
        obj.insert(key.to_string(), value);
    }

    Ok(changed)
}

/// 将旧的顶层 max-tokens 迁移到每个模型的 max-output-tokens。
fn migrate_max_tokens(root: &mut serde_json::Map<String, Value>) -> Result<bool> {
    let Some(max_tokens) = root.remove("max-tokens") else {
        return Ok(false);
    };

    if let Some(models) = root.get_mut("models").and_then(|v| v.as_array_mut()) {
        for model in models.iter_mut() {
            let Some(obj) = model.as_object_mut() else {
                return Err(anyhow!("models must be migrated to objects before max-tokens"));
            };
            obj.entry("max-output-tokens".to_string())
                .or_insert_with(|| max_tokens.clone());
        }
    }

    Ok(true)
}

/// CLI 入口函数：解析参数并分发到对应子命令
fn main() {
    let mut argv: Vec<String> = std::env::args().collect();

    if argv.get(1).map(|s| s == "fcplug").unwrap_or(false) {
        argv.remove(1);
    }

    let cli = Cli::parse_from(argv);

    let res = match cli.command {
        Commands::Init(args) => run_init(args.parent_dir),
        Commands::Build(args) => run_build(args),
        Commands::Update(args) => run_update(args),
    };
    if let Err(e) = res {
        error(&format!("{:#}", e));
        std::process::exit(1);
    }
}

/// 在插件项目目录下生成 `manifest.json`
fn write_manifest(root: &Path, id: &str, kind: &str, author: &str, desc: &str) -> Result<()> {
    let mut manifest = serde_json::json!({
        "meta": {
            "id": id,
            "name": id,
            "version": "0.1.0",
            "author": author,
            "description": desc,
            "agreement-version": AGREEMENT_VERSION,
            "kind": kind,
            "url": "https://api.example.com/v1"
        },
        "models": [
            {
                "id": "model-1",
                "name": "Model 1"
            }
        ]
    });

    // 按类型添加默认扩展字段
    match kind {
        "llm" => {
            manifest["default-model"] = serde_json::json!("model-1");
            manifest["default-supports"] = serde_json::json!({
                "thinking": false,
                "tools": true,
                "stream": true,
                "vision-input": false,
                "structured-output": false
            });
            manifest["default-thinking-efforts"] = serde_json::json!([]);
        }
        "tts" => {
            manifest["voices"] = serde_json::json!([
                {
                    "id": "default-voice",
                    "name": "Default Voice",
                    "language": ["Chinese", "English"]
                }
            ]);
            manifest["default-model"] = serde_json::json!("model-1");
            manifest["default-voice"] = serde_json::json!("default-voice");
            manifest["supported-formats"] = serde_json::json!(["mp3", "wav"]);
            manifest["supported-languages"] = serde_json::json!(["Chinese", "English"]);
            manifest["max-characters"] = serde_json::json!(10000);
            manifest["supports-emotion"] = serde_json::json!(false);
            manifest["supports-voice-modify"] = serde_json::json!(false);
            manifest["supports-ssml"] = serde_json::json!(false);
        }
        "image" => {
            manifest["default-model"] = serde_json::json!("model-1");
            manifest["supported-sizes"] = serde_json::json!(["1024x1024"]);
            manifest["supported-formats"] = serde_json::json!(["png"]);
            manifest["supports-negative-prompt"] = serde_json::json!(false);
            manifest["supports-image-to-image"] = serde_json::json!(false);
            manifest["max-batch-size"] = serde_json::json!(1);
        }
        _ => {}
    }

    fs::write(
        root.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    Ok(())
}

/// 在插件项目目录下生成 `Cargo.toml`
fn write_cargo(root: &Path, id: &str) -> Result<()> {
    let content = format!(
        r#"[package]
name = "fcplug_{}"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = {{ version = "0.53.1", features = ["macros"] }}
serde_json = "1.0"
"#,
        id.replace('-', "_")
    );

    fs::write(root.join("Cargo.toml"), content)?;
    Ok(())
}

/// 在插件项目目录下生成 `src/lib.rs`（包含默认 WIT 绑定和 Guest 实现）
fn write_lib(root: &Path) -> Result<()> {
    let code = r#"wit_bindgen::generate!({
    path: "wit/plugin.wit",
    world: "api",
});

use crate::exports::mapper::plugin::mapper::Guest;

struct MyPlugin;

impl Guest for MyPlugin {

    fn map_request(input: String) -> String {
        input
    }

    fn map_response(input: String) -> String {
        input
    }

    fn map_stream_line(input: String) -> String {
        input
    }
}

export!(MyPlugin);
"#;
    fs::write(root.join("src/lib.rs"), code)?;
    Ok(())
}

/// 在插件项目目录下复制默认 WIT 接口定义到 `wit/plugin.wit`
fn write_wit(root: &Path) -> Result<()> {
    fs::write(root.join("wit/plugin.wit"), DEFAULT_WIT)?;
    Ok(())
}

/// 在插件项目目录下复制默认图标到 `icon.png`
fn write_icon(root: &Path) -> Result<()> {
    fs::write(root.join("icon.png"), DEFAULT_ICON)?;
    Ok(())
}

/// 在插件项目目录下复制默认 README 模板到 `README.md`
fn write_readme(root: &Path) -> Result<()> {
    fs::write(root.join("README.md"), DEFAULT_README)?;
    Ok(())
}

/// 在插件项目目录下生成 `.gitignore`
fn write_gitignore(root: &Path) -> Result<()> {
    fs::write(root.join(".gitignore"), "/target\n/dist\n")?;
    Ok(())
}
