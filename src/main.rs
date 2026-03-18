use anyhow::{Result, anyhow};
use clap::Parser;
use serde::Deserialize;
use serde_json::Value;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_GREEN: &str = "\x1b[32m";
const COLOR_YELLOW: &str = "\x1b[33m";
const COLOR_RED: &str = "\x1b[31m";

fn info(msg: &str) {
    println!("{COLOR_GREEN}[INFO]{COLOR_RESET} {msg}");
}

fn warning(msg: &str) {
    println!("{COLOR_YELLOW}[WARN]{COLOR_RESET} {msg}");
}

fn error(msg: &str) {
    eprintln!("{COLOR_RED}[ERROR]{COLOR_RESET} {msg}");
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
    #[arg(long)]
    path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Manifest {
    id: String,
    name: String,
    author: String,
    #[allow(dead_code)]
    description: String,
    version: String,
    kind: String,
    abi_version: u32,
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
    info("== Building WASM plugin ==");

    let status = Command::new("cargo")
        .args(["build", "--target", "wasm32-wasip2", "--release"])
        .status()?;

    if !status.success() {
        return Err(anyhow!("cargo build failed"));
    }

    Ok(())
}

fn find_wasm() -> Result<PathBuf> {
    // 从 Cargo.toml 读 crate name，替换 '-' 为 '_'
    let cargo_toml: Value = toml::from_str(&fs::read_to_string("Cargo.toml")?)?;
    let crate_name = cargo_toml["package"]["name"]
        .as_str()
        .ok_or(anyhow!("missing package.name"))?
        .replace('-', "_");

    let expected = Path::new("target/wasm32-wasip2/release").join(format!("{}.wasm", crate_name));

    if expected.exists() {
        Ok(expected)
    } else {
        Err(anyhow!("expected wasm not found: {}", expected.display()))
    }
}

fn optimize_wasm(wasm: &Path) -> Result<()> {
    info("== Running wasm-opt ==");

    let status = Command::new("wasm-opt")
        .args(["-Oz"])
        .arg(wasm)
        .args(["-o"])
        .arg(wasm)
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        _ => {
            warning("Warning: wasm-opt not found or failed, skipping optimization");
            Ok(())
        }
    }
}

fn validate_manifest() -> Result<Manifest> {
    info("== Validating manifest.json ==");

    let mut file = File::open("manifest.json").map_err(|_| anyhow!("manifest.json not found"))?;

    let mut buf = String::new();
    file.read_to_string(&mut buf)?;

    let manifest: Manifest =
        serde_json::from_str(&buf).map_err(|e| anyhow!("invalid manifest.json: {}", e))?;

    if manifest.id.trim().is_empty() {
        return Err(anyhow!("manifest.id cannot be empty"));
    }
    if manifest.name.trim().is_empty() {
        return Err(anyhow!("manifest.name cannot be empty"));
    }
    if manifest.version.trim().is_empty() {
        return Err(anyhow!("manifest.version cannot be empty"));
    }

    if manifest.author.trim().is_empty() {
        return Err(anyhow!("manifest.author cannot be empty"));
    }

    if !["kind/llm", "kind/image", "kind/tts"].contains(&manifest.kind.as_str()) {
        return Err(anyhow!(
            "manifest.kind must be one of: kind/llm, kind/image, kind/tts (got: {})",
            manifest.kind
        ));
    }

    if manifest.abi_version != ABI_VERSION {
        return Err(anyhow!("manifest.abi_version must be {}", ABI_VERSION));
    }

    println!(
        "Plugin: {} ({}) v{}",
        manifest.name, manifest.id, manifest.version
    );

    Ok(manifest)
}

fn check_icon() -> Result<PathBuf> {
    let icon = Path::new("icon.png");

    if !icon.exists() {
        println!("== Generating icon.png ==");

        fs::write(icon, DEFAULT_ICON)?;
    }

    let (w, h) = read_png_size(icon)?;

    if w > 128 || h > 128 {
        return Err(anyhow!("icon.png must be ≤ 128x128"));
    }

    if w != h {
        return Err(anyhow!("icon must be square"));
    }

    Ok(icon.to_path_buf())
}

fn package(wasm: &Path, icon: PathBuf, plugin_id: &str) -> Result<()> {
    println!("== Preparing dist folder ==");

    let dist = Path::new("dist");

    if dist.exists() {
        fs::remove_dir_all(dist)?;
    }

    fs::create_dir_all(dist)?;

    let fcplug_filename = format!("{}.fcplug", plugin_id);

    println!("== Packing fcplug ==");

    let fcplug = File::create(dist.join(&fcplug_filename))?;
    let mut zip = ZipWriter::new(fcplug);

    let opt = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("manifest.json", opt)?;
    std::io::copy(&mut File::open("manifest.json")?, &mut zip)?;

    zip.start_file("plugin.wasm", opt)?;
    std::io::copy(&mut File::open(wasm)?, &mut zip)?;

    zip.start_file("icon.png", opt)?;
    std::io::copy(&mut File::open(icon)?, &mut zip)?;

    zip.finish()?;

    println!();
    println!("Build complete:");
    println!("dist/{}", fcplug_filename);

    Ok(())
}

fn read_png_size(path: &Path) -> Result<(u32, u32)> {
    use std::io::Read;

    let mut f = File::open(path)?;
    let mut buf = [0u8; 24];
    f.read_exact(&mut buf)?;

    if &buf[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(anyhow!("not a png"));
    }

    let w = u32::from_be_bytes(buf[16..20].try_into()?);
    let h = u32::from_be_bytes(buf[20..24].try_into()?);

    Ok((w, h))
}

fn run_init(path: Option<String>) -> Result<()> {
    println!("== Create new FlowCloudAI plugin ==");

    let id = ask("Plugin id", "my-plugin")?;
    let mut kind: String;
    loop {
        kind = ask("Plugin kind (llm|image|tts)", "llm")?;
        if kind != "llm" && kind != "image" && kind != "tts" {
            error("Invalid plugin kind!");
        } else {
            break;
        }
    }
    let author = ask("Author", "unknown")?;
    let description = ask("Description", "example plugin")?;

    let base = path.unwrap_or_else(|| ".".into());
    let root = Path::new(&base).join(format!("fcplug-{}", id));

    if root.exists() {
        return Err(anyhow!("directory already exists"));
    }

    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("wit"))?;

    write_manifest(&root, &id, &kind, &author, &description)?;
    write_cargo(&root, &id)?;
    write_lib(&root)?;
    write_wit(&root)?;
    write_icon(&root)?;
    write_readme(&root)?;
    write_gitignore(&root)?;

    println!();
    info("Plugin scaffold created:");
    println!("{}", root.display());

    Ok(())
}

fn run_build(args: BuildArgs) -> Result<()> {
    // 0. 检测 Cargo.toml
    if !Path::new("Cargo.toml").exists() {
        return Err(anyhow!(
            "Cargo.toml not found. Run this command from the plugin project root."
        ));
    }

    // 1. 校验 manifest
    let manifest = validate_manifest()?;

    // 2. 编译 wasm
    if !args.no_build {
        build_wasm()?;
    }

    // 3. 找到 wasm 文件
    let wasm = find_wasm()?;

    // 4. wasm-opt 压缩
    if !args.no_opt {
        optimize_wasm(&wasm)?;
    }

    // 5. 检查 icon
    let icon = check_icon()?;

    // 6. 打包
    package(&wasm, icon, &manifest.id)?;

    Ok(())
}

fn main() {
    let mut argv: Vec<String> = std::env::args().collect();

    if argv.get(1).map(|s| s == "fcplug").unwrap_or(false) {
        argv.remove(1);
    }

    let cli = Cli::parse_from(argv);

    let res = match cli.command {
        Commands::Init(args) => run_init(args.path),
        Commands::Build(args) => run_build(args),
    };
    if let Err(e) = res {
        error(&e.to_string());
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
        "url":"server URL",
        "module-list":[
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