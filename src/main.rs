use anyhow::{Result, anyhow};
use clap::Parser;
use serde::Deserialize;
use serde_json::Value;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;
use url::Url;

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_GREEN: &str = "\x1b[32m";
const COLOR_YELLOW: &str = "\x1b[33m";
const COLOR_RED: &str = "\x1b[31m";
const COLOR_CYAN: &str = "\x1b[36m";
const COLOR_DIM: &str = "\x1b[2m";

fn info(msg: &str) {
    println!("{COLOR_GREEN}[INFO]{COLOR_RESET} {msg}");
}

fn detail(msg: &str) {
    println!("{COLOR_DIM}      {msg}{COLOR_RESET}");
}

fn step(index: u32, total: u32, msg: &str) {
    println!(
        "\n{COLOR_CYAN}[{}/{}]{COLOR_RESET} {msg}",
        index, total
    );
}

fn warning(msg: &str) {
    println!("{COLOR_YELLOW}[WARN]{COLOR_RESET} {msg}");
}

fn error(msg: &str) {
    eprintln!("{COLOR_RED}[ERROR]{COLOR_RESET} {msg}");
}

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

fn elapsed_str(start: Instant) -> String {
    let d = start.elapsed();
    if d.as_secs() >= 1 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}ms", d.as_millis())
    }
}

/// Validate a plugin id: non-empty, lowercase ASCII alphanumeric + hyphens,
/// must start with a letter, max 64 characters.
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

/// Validate author: non-empty, printable ASCII, max 128 characters.
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

/// Validate description: max 100 characters.
fn validate_description(description: &str) -> Result<()> {
    if description.len() > 100 {
        return Err(anyhow!(
            "description is too long ({} chars, max 1024)",
            description.len()
        ));
    }
    Ok(())
}

/// Validate version string: basic semver format (x.y.z).
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

/// Validate URL: must be http or https.
fn validate_url(raw: &str) -> Result<()> {
    let parsed = Url::parse(raw)
        .map_err(|e| anyhow!("invalid URL \"{}\": {}", raw, e))?;

    match parsed.scheme() {
        "http" | "https" => Ok(()),
        s => Err(anyhow!("URL scheme must be http or https, got \"{}\"", s)),
    }
}

const ABI_VERSION: u32 = 1;
static DEFAULT_ICON: &[u8] = include_bytes!("../src/templates/icon.png");
static DEFAULT_README: &[u8] = include_bytes!("../src/templates/README.md");
static DEFAULT_WIT: &[u8] = include_bytes!("../src/templates/plugin.wit");

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

#[derive(clap::Subcommand)]
enum Commands {
    Build(BuildArgs),
    Init(InitArgs),
}

#[derive(Parser)]
#[command(about = "Build and package a plugin into .fcplug")]
struct BuildArgs {
    #[arg(long)]
    no_build: bool,

    #[arg(long)]
    no_opt: bool,
}

#[derive(Parser)]
#[command(about = "Create a new plugin project scaffold")]
struct InitArgs {
    /// Parent directory where the plugin project folder will be created
    #[arg(long)]
    parent_dir: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Manifest {
    id: String,
    name: String,
    author: String,
    #[allow(unused)]
    description: Option<String>,
    version: String,
    kind: String,
    abi_version: u32,
    url: String,
    model_list: Vec<String>,
}

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

fn optimize_wasm(wasm: &Path) -> Result<()> {
    let size_before = fs::metadata(wasm)?.len();
    let start = Instant::now();
    let tmp_path = wasm.with_extension("wasm.opt.tmp");

    // wasm-tools strip: remove debug info, custom sections, etc.
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
            let code = s.code()
                .map(|c| format!("exit code {}", c))
                .unwrap_or_else(|| "killed by signal".into());
            warning(&format!("wasm-tools strip failed ({}), skipping", code));
            Ok(())
        }
        Err(e) => {
            warning(&format!("wasm-tools not found ({}), skipping optimization", e));
            detail("Install: cargo install wasm-tools");
            Ok(())
        }
    }
}

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

    let manifest: Manifest =
        serde_json::from_str(&buf).map_err(|e| {
            let line = e.line();
            let col = e.column();
            let loc = format!(" at line {}, column {}", line, col);
            anyhow!("invalid manifest.json{}: {}", loc, e)
        })?;

    // Field validations with specific guidance
    validate_plugin_id(&manifest.id).map_err(|e| {
        anyhow!(
            "manifest.id is invalid: {}\n       Example: \"id\": \"my-awesome-plugin\"",
            e
        )
    })?;

    if manifest.name.trim().is_empty() {
        return Err(anyhow!(
            "manifest.name cannot be empty\n       Example: \"name\": \"My Plugin\""
        ));
    }

    validate_version(&manifest.version).map_err(|e| {
        anyhow!(
            "manifest.version is invalid: {}\n       Example: \"version\": \"0.1.0\"",
            e
        )
    })?;

    validate_author(&manifest.author).map_err(|e| {
        anyhow!(
            "manifest.author is invalid: {}\n       Example: \"author\": \"yourname\"",
            e
        )
    })?;

    validate_url(&manifest.url).map_err(|e| {
        anyhow!(
            "manifest.author is invalid:{}\n       Example: \"url\": \"https://example.com/my-plugin\"",
            e
        )
    })?;

    if !manifest.url.starts_with("http://") && !manifest.url.starts_with("https://") {
        return Err(anyhow!(
            "manifest.url must be a valid HTTPS/HTTP URL\n       Example: \"url\": \"https://example.com/my-plugin\""
        ));
    }
    if manifest.model_list.is_empty() {
        return Err(anyhow!(
            "manifest.model-list cannot be empty\n       Example: \"model-list\": [\"deepseek-chat\"]"
        ));
    }

    let valid_kinds = ["kind/llm", "kind/image", "kind/tts"];
    if !valid_kinds.contains(&manifest.kind.as_str()) {
        return Err(anyhow!(
            "manifest.kind is \"{}\", must be one of: {}\n       Example: \"kind\": \"kind/llm\"",
            manifest.kind,
            valid_kinds.join(", ")
        ));
    }

    if manifest.abi_version != ABI_VERSION {
        return Err(anyhow!(
            "manifest.abi-version is {}, expected {}\n       Hint: update to the current ABI version",
            manifest.abi_version,
            ABI_VERSION
        ));
    }

    info(&format!(
        "Manifest OK: {} ({}) v{} [{}]",
        manifest.name, manifest.id, manifest.version, manifest.kind
    ));
    detail(&format!("Author: {}", manifest.author));

    Ok(manifest)
}

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

fn run_build(args: BuildArgs) -> Result<()> {
    let total_start = Instant::now();

    let total_steps = 3 + (!args.no_build as u32) + (!args.no_opt as u32);
    let mut current_step: u32 = 0;

    // 0. Cargo.toml check
    if !Path::new("Cargo.toml").exists() {
        return Err(anyhow!(
            "Cargo.toml not found in current directory: {}\n       Hint: run this command from the plugin project root",
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<unknown>".into())
        ));
    }

    // 1. Validate manifest
    current_step += 1;
    step(current_step, total_steps, "Validating manifest.json");
    let manifest = validate_manifest()?;

    // 2. Build WASM
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

    // 3. Locate artifact
    current_step += 1;
    step(current_step, total_steps, "Locating WASM artifact");
    let wasm = find_wasm()?;

    // 4. Optimize
    if !args.no_opt {
        current_step += 1;
        step(current_step, total_steps, "Optimizing WASM binary");
        optimize_wasm(&wasm)?;
    } else {
        info("Skipping optimization (--no-opt)");
    }

    // 5. Check icon
    current_step += 1;
    step(current_step, total_steps, "Checking icon & packaging");
    let icon = check_icon()?;

    // 6. Package
    package(&wasm, &icon, &manifest.id)?;

    println!();
    println!(
        "{COLOR_GREEN}✓ Build complete in {}{COLOR_RESET}",
        elapsed_str(total_start)
    );

    Ok(())
}

fn main() {
    let mut argv: Vec<String> = std::env::args().collect();

    if argv.get(1).map(|s| s == "fcplug").unwrap_or(false) {
        argv.remove(1);
    }

    let cli = Cli::parse_from(argv);

    let res = match cli.command {
        Commands::Init(args) => run_init(args.parent_dir),
        Commands::Build(args) => run_build(args),
    };
    if let Err(e) = res {
        error(&format!("{:#}", e));
        std::process::exit(1);
    }
}

fn write_manifest(root: &Path, id: &str, kind: &str, author: &str, desc: &str) -> Result<()> {
    let manifest = serde_json::json!({
        "id": id,
        "name": id,
        "version": "0.1.0",
        "author": author,
        "description": desc,
        "kind": format!("kind/{}", kind),
        "abi-version": 1,
        "url":"https://api.example.com/v1",
        "model-list":[
            "module1",
            "module2"
        ]
    });

    fs::write(
        root.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    Ok(())
}

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

fn write_wit(root: &Path) -> Result<()> {
    fs::write(root.join("wit/plugin.wit"), DEFAULT_WIT)?;
    Ok(())
}

fn write_icon(root: &Path) -> Result<()> {
    fs::write(root.join("icon.png"), DEFAULT_ICON)?;
    Ok(())
}

fn write_readme(root: &Path) -> Result<()> {
    fs::write(root.join("README.md"), DEFAULT_README)?;
    Ok(())
}

fn write_gitignore(root: &Path) -> Result<()> {
    fs::write(root.join(".gitignore"), "/target\n/dist\n")?;
    Ok(())
}