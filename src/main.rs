use anyhow::{Result, anyhow};
use clap::Parser;
use image::GenericImageView;
use serde::Deserialize;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const ABI_VERSION: u32 = 1;
static DEFAULT_ICON: &[u8] = include_bytes!("../src/templates/icon.png");
static DEFAULT_README: &[u8] = include_bytes!("../src/templates/README.md");
static DEFAULT_WIT: &[u8] = include_bytes!("../src/templates/plugin.wit");
static DEFAULT_TYPES: &[u8] = include_bytes!("../src/templates/types.rs");

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
#[allow(dead_code)]
#[serde(rename_all = "kebab-case")]
struct Manifest {
    id: String,
    name: String,
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
    println!("== Building WASM plugin ==");

    let status = Command::new("cargo")
        .args(["build", "--target", "wasm32-wasip2", "--release"])
        .status()?;

    if !status.success() {
        return Err(anyhow!("cargo build failed"));
    }

    Ok(())
}

fn find_wasm() -> Result<PathBuf> {
    let dir = Path::new("target/wasm32-wasip2/release");

    for entry in fs::read_dir(dir)? {
        let path = entry?.path();

        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("wasm") {
            return Ok(path);
        }
    }

    Err(anyhow!("no wasm file found"))
}

fn optimize_wasm(wasm: &Path) -> Result<()> {
    println!("== Running wasm-opt ==");

    let status = Command::new("wasm-opt")
        .args(["-Oz"])
        .arg(wasm)
        .args(["-o"])
        .arg(wasm)
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        _ => {
            println!("Warning: wasm-opt not found or failed, skipping optimization");
            Ok(())
        }
    }
}

fn validate_manifest() -> Result<Manifest> {
    println!("== Validating manifest.json ==");

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

    if manifest.kind.trim().is_empty() {
        return Err(anyhow!("manifest.kind cannot be empty"));
    }
    if manifest.abi_version != ABI_VERSION {
        return Err(anyhow!(
            "manifest.abi_version must be {}",
            ABI_VERSION
        ));
    }

    println!(
        "Plugin: {} ({}) v{}",
        manifest.name, manifest.id, manifest.version
    );

    Ok(manifest)
}

fn check_icon() -> Result<Option<PathBuf>> {
    let icon = Path::new("icon.png");

    if !icon.exists() {
        return Ok(None);
    }

    let img = image::open(icon)?;
    let (w, h) = img.dimensions();

    if w > 128 || h > 128 {
        return Err(anyhow!("icon.png must be ≤ 128x128"));
    }

    if w != h {
        return Err(anyhow!("icon must be square"));
    }

    Ok(Some(icon.to_path_buf()))
}

fn package(wasm: &Path, icon: Option<PathBuf>) -> Result<()> {
    println!("== Preparing dist folder ==");

    let dist = Path::new("dist");

    if dist.exists() {
        fs::remove_dir_all(dist)?;
    }

    fs::create_dir_all(dist)?;

    let wasm_dst = dist.join("plugin.wasm");
    fs::copy(wasm, &wasm_dst)?;

    fs::copy("manifest.json", dist.join("manifest.json"))?;

    println!("== Packing fcplug ==");

    let fcplug = File::create(dist.join("plugin.fcplug"))?;
    let mut zip = ZipWriter::new(fcplug);

    let opt = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("manifest.json", opt)?;
    std::io::copy(&mut File::open("manifest.json")?, &mut zip)?;

    zip.start_file("plugin.wasm", opt)?;
    std::io::copy(&mut File::open(&wasm_dst)?, &mut zip)?;

    if let Some(icon_path) = icon {
        zip.start_file("icon.png", opt)?;
        std::io::copy(&mut File::open(icon_path)?, &mut zip)?;
    }

    zip.finish()?;

    println!();
    println!("Build complete:");
    println!("dist/plugin.fcplug");

    Ok(())
}

fn run_init(path: Option<String>) -> Result<()> {
    println!("== Create new FlowCloudAI plugin ==");

    let id = ask("Plugin id", "my-plugin")?;
    let kind = ask("Plugin kind (llm|image|tts)", "llm")?;
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
    write_types(&root)?;
    write_wit(&root)?;
    write_icon(&root)?;
    write_readme(&root)?;

    println!();
    println!("Plugin scaffold created:");
    println!("{}", root.display());

    Ok(())
}

fn run_build(args: BuildArgs) -> Result<()> {
    // 1. 校验 manifest
    let _manifest = validate_manifest()?;

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
    package(&wasm, icon)?;

    Ok(())
}

fn main() -> Result<()> {
    let mut argv: Vec<String> = std::env::args().collect();

    if argv.get(1).map(|s| s == "fcplug").unwrap_or(false) {
        argv.remove(1);
    }

    let cli = Cli::parse_from(argv);

    match cli.command {
        Commands::Init(args) => run_init(args.path),
        Commands::Build(args) => run_build(args),
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
        "abi_version": 1
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
name = "fcplug-{}"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = {{ version = "0.53.1", features = ["macros"] }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"
"#,
        id
    );

    fs::write(root.join("Cargo.toml"), content)?;
    Ok(())
}

fn write_lib(root: &Path) -> Result<()> {
    let code = format!(
        r#"mod types;

wit_bindgen::generate!({{
    path: "wit/plugin.wit",
    world: "api",
}});

use crate::exports::mapper::plugin::mapper::Guest;

struct MyPlugin;

impl Guest for MyPlugin {{

    fn map_request(input: String) -> String {{
        input
    }}

    fn map_response(input: String) -> String {{
        input
    }}
}}

export!(MyPlugin);
"#);
    fs::write(root.join("src/lib.rs"), code)?;
    Ok(())
}

fn write_types(root: &Path) -> Result<()> {
    fs::write(root.join("src/types.rs"), DEFAULT_TYPES)?;
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
