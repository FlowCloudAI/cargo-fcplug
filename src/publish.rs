//! `cargo fcplug publish`：把本地 `.fcplug` 发布到 FlowCloudAI 插件市场。
//!
//! 流程：本地解析（候选信息）→ 创建上传会话 → 直传 COS（文件不经网站服务器）
//! → 服务端校验发布。服务端会重新解析并校验插件，本地解析结果不被信任。
//!
//! 安全约束：不接触 COS 长期密钥；不打印预签名 URL 的签名参数；
//! 管理员密码只经环境变量或交互输入，不进命令行参数。

use crate::{AGREEMENT_VERSION, KINDS, Manifest, detail, human_size, info, step, warning};
use anyhow::{Context, Result, anyhow};
use base64::Engine as _;
use md5::Md5;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `publish` 子命令参数
#[derive(clap::Parser)]
#[command(about = "Publish a .fcplug package to the FlowCloudAI plugin market")]
pub struct PublishArgs {
    /// 指定要发布的 .fcplug 文件（默认：dist/{manifest.meta.id}.fcplug）
    #[arg(long)]
    pub file: Option<PathBuf>,

    /// 插件市场服务器地址
    #[arg(long, default_value = "https://www.flowcloudai.cn")]
    pub server: String,

    /// 跳过交互确认（CI 场景）
    #[arg(long)]
    pub yes: bool,
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// 服务端 .fcplug 大小上限，本地提前拦截（服务端仍会校验）
const MAX_FCPLUG_BYTES: u64 = 10 * 1024 * 1024;

// ─── 服务端响应结构 ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct UploadSessionResp {
    upload_id: String,
    method: String,
    upload_url: String,
    headers: UploadHeaders,
    expires_at: String,
}

#[derive(Deserialize)]
struct UploadHeaders {
    #[serde(rename = "Content-Type")]
    content_type: String,
    #[serde(rename = "Content-MD5")]
    content_md5: String,
}

#[derive(Deserialize)]
struct PublishOutcome {
    id: String,
    version: String,
    status: String,
    download_url: String,
}

// ─── 本地解析 ────────────────────────────────────────────────────────────────

struct LocalPackage {
    path: PathBuf,
    bytes: Vec<u8>,
    manifest: Manifest,
    sha256_hex: String,
    md5_base64: String,
}

/// 未指定 --file 时：读当前目录 manifest.json 的 id，定位 dist/{id}.fcplug。
fn default_package_path(project_dir: &Path) -> Result<PathBuf> {
    let manifest_path = project_dir.join("manifest.json");
    let raw = std::fs::read_to_string(&manifest_path).map_err(|_| {
        anyhow!(
            "当前目录没有 manifest.json\n       Hint: 在插件项目根目录运行，或用 --file 指定 .fcplug 文件"
        )
    })?;
    let manifest: Manifest =
        serde_json::from_str(&raw).map_err(|e| anyhow!("manifest.json 解析失败：{e}"))?;
    let path = project_dir
        .join("dist")
        .join(format!("{}.fcplug", manifest.meta.id));
    if !path.exists() {
        return Err(anyhow!(
            "未找到 {}\n       Hint: 请先运行 cargo fcplug build",
            path.display()
        ));
    }
    Ok(path)
}

/// 本地解析 .fcplug：ZIP 合法性、manifest/plugin.wasm 存在性、元数据与摘要。
/// 结果仅作为候选信息展示与会话声明，服务端会重新校验。
fn parse_package(path: &Path) -> Result<LocalPackage> {
    let bytes =
        std::fs::read(path).with_context(|| format!("无法读取 {}", path.display()))?;
    if bytes.len() as u64 > MAX_FCPLUG_BYTES {
        return Err(anyhow!(
            ".fcplug 大小 {} 超出上限 {}",
            human_size(bytes.len() as u64),
            human_size(MAX_FCPLUG_BYTES)
        ));
    }

    let cursor = std::io::Cursor::new(&bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|_| anyhow!("{} 不是有效的 ZIP/.fcplug 文件", path.display()))?;
    let mut manifest_data = Vec::new();
    archive
        .by_name("manifest.json")
        .map_err(|_| anyhow!("插件包缺少 manifest.json"))?
        .read_to_end(&mut manifest_data)?;
    archive
        .by_name("plugin.wasm")
        .map_err(|_| anyhow!("插件包缺少 plugin.wasm"))?;

    let manifest: Manifest = serde_json::from_slice(&manifest_data)
        .map_err(|e| anyhow!("manifest.json 解析失败：{e}"))?;

    if manifest.meta.agreement_version != AGREEMENT_VERSION {
        return Err(anyhow!(
            "协议版本不支持：期望 {AGREEMENT_VERSION}，实际 {}\n       Hint: 运行 cargo fcplug update 迁移 manifest",
            manifest.meta.agreement_version
        ));
    }
    if !KINDS.contains(&manifest.meta.kind.as_str()) {
        return Err(anyhow!(
            "插件类型不合法：{}（允许 {}）",
            manifest.meta.kind,
            KINDS.join(" / ")
        ));
    }

    let sha256_hex = format!("{:x}", Sha256::digest(&bytes));
    let md5_base64 = base64::engine::general_purpose::STANDARD.encode(Md5::digest(&bytes));

    Ok(LocalPackage {
        path: path.to_path_buf(),
        bytes,
        manifest,
        sha256_hex,
        md5_base64,
    })
}

// ─── HTTP 辅助 ───────────────────────────────────────────────────────────────

/// 从服务端错误响应体提取 `{"error": "..."}` 的可读信息。
fn error_from_body(status: u16, body: &str) -> anyhow::Error {
    let msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|e| e.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.trim().chars().take(300).collect());
    if msg.is_empty() {
        anyhow!("HTTP {status}")
    } else {
        anyhow!("HTTP {status}：{msg}")
    }
}

fn api_error(err: ureq::Error) -> anyhow::Error {
    match err {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            error_from_body(code, &body)
        }
        other => anyhow!("网络请求失败：{other}"),
    }
}

/// 打印 URL 时去掉 query（预签名的签名参数不进日志）。
fn display_url(raw: &str) -> String {
    match url::Url::parse(raw) {
        Ok(parsed) => format!(
            "{}://{}{}",
            parsed.scheme(),
            parsed.host_str().unwrap_or_default(),
            parsed.path()
        ),
        Err(_) => "<无法解析的 URL>".to_string(),
    }
}

// ─── 认证 ────────────────────────────────────────────────────────────────────

/// 凭据来源：优先环境变量（CI），否则交互输入。密码不回显、不进命令行参数。
fn resolve_credentials() -> Result<(String, String)> {
    let env_user = std::env::var("FLOWCLOUDAI_PUBLISH_USERNAME").ok();
    let env_pass = std::env::var("FLOWCLOUDAI_PUBLISH_PASSWORD").ok();
    if let (Some(user), Some(pass)) = (env_user, env_pass)
        && !user.trim().is_empty()
        && !pass.is_empty()
    {
        info("使用环境变量中的发布者凭据");
        return Ok((user.trim().to_string(), pass));
    }

    print!("发布者用户名: ");
    std::io::stdout().flush()?;
    let mut username = String::new();
    std::io::stdin().read_line(&mut username)?;
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err(anyhow!("用户名不能为空"));
    }
    let password = rpassword::prompt_password("发布者密码: ")?;
    if password.is_empty() {
        return Err(anyhow!("密码不能为空"));
    }
    Ok((username, password))
}

fn login(agent: &ureq::Agent, server: &str) -> Result<String> {
    let (username, password) = resolve_credentials()?;
    let resp = agent
        .post(&format!("{server}/api/admin/auth/login"))
        .timeout(REQUEST_TIMEOUT)
        .send_json(serde_json::json!({ "username": username, "password": password }))
        .map_err(|e| match e {
            ureq::Error::Status(401, _) => anyhow!("登录失败：用户名或密码错误"),
            ureq::Error::Status(403, _) => {
                anyhow!("登录失败：账号被锁定或无权限，请稍后再试")
            }
            other => api_error(other),
        })?;
    let payload: serde_json::Value = resp.into_json()?;
    payload
        .get("access_token")
        .and_then(|token| token.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("登录响应缺少 access_token"))
}

// ─── 发布流程 ────────────────────────────────────────────────────────────────

fn create_session(
    agent: &ureq::Agent,
    server: &str,
    token: &str,
    package: &LocalPackage,
) -> Result<UploadSessionResp> {
    let meta = &package.manifest.meta;
    let resp = agent
        .post(&format!("{server}/api/admin/plugins/uploads"))
        .timeout(REQUEST_TIMEOUT)
        .set("Authorization", &format!("Bearer {token}"))
        .send_json(serde_json::json!({
            "plugin": {
                "id": meta.id,
                "name": meta.name,
                "version": meta.version,
                "author": meta.author,
                "kind": meta.kind,
                "agreement_version": meta.agreement_version,
            },
            "file_size": package.bytes.len(),
            "sha256": package.sha256_hex,
            "content_md5": package.md5_base64,
        }))
        .map_err(api_error)?;
    Ok(resp.into_json::<UploadSessionResp>()?)
}

/// 包装 Read 以打印上传进度（单行刷新）。
struct ProgressReader<R> {
    inner: R,
    total: u64,
    sent: u64,
    last_percent: u64,
}

impl<R: Read> Read for ProgressReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.sent += n as u64;
        let percent = if self.total == 0 {
            100
        } else {
            self.sent * 100 / self.total
        };
        if percent != self.last_percent {
            self.last_percent = percent;
            print!("\r      上传进度：{percent}%");
            let _ = std::io::stdout().flush();
        }
        Ok(n)
    }
}

fn upload_to_cos(agent: &ureq::Agent, session: &UploadSessionResp, bytes: &[u8]) -> Result<()> {
    if session.method != "PUT" {
        return Err(anyhow!("服务端返回了不支持的上传方式：{}", session.method));
    }
    detail(&format!("目标：{}", display_url(&session.upload_url)));
    detail(&format!("凭证有效期至：{}", session.expires_at));

    let reader = ProgressReader {
        inner: bytes,
        total: bytes.len() as u64,
        sent: 0,
        last_percent: u64::MAX,
    };
    let result = agent
        .put(&session.upload_url)
        .timeout(UPLOAD_TIMEOUT)
        .set("Content-Type", &session.headers.content_type)
        .set("Content-MD5", &session.headers.content_md5)
        .set("Content-Length", &bytes.len().to_string())
        .send(reader);
    println!();

    match result {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(403, _)) => Err(anyhow!(
            "COS 拒绝上传（HTTP 403）：上传凭证可能已过期，请重新执行 cargo fcplug publish"
        )),
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            Err(anyhow!(
                "COS 上传失败：HTTP {code} {}",
                body.trim().chars().take(200).collect::<String>()
            ))
        }
        Err(other) => Err(anyhow!("COS 上传失败：{other}")),
    }
}

/// complete 幂等：网络错误与「校验中」冲突自动重试。
fn complete_with_retry(
    agent: &ureq::Agent,
    server: &str,
    token: &str,
    upload_id: &str,
) -> Result<PublishOutcome> {
    let url = format!("{server}/api/admin/plugins/uploads/{upload_id}/complete");
    let mut attempts_left = 4u32;
    loop {
        attempts_left -= 1;
        let result = agent
            .post(&url)
            .timeout(REQUEST_TIMEOUT)
            .set("Authorization", &format!("Bearer {token}"))
            .call();
        match result {
            Ok(resp) => return Ok(resp.into_json::<PublishOutcome>()?),
            Err(ureq::Error::Status(401, _)) => {
                return Err(anyhow!("登录已过期，请重新执行 cargo fcplug publish"));
            }
            Err(ureq::Error::Status(409, resp)) if attempts_left > 0 => {
                let body = resp.into_string().unwrap_or_default();
                warning(&format!(
                    "{}，3 秒后重试",
                    error_from_body(409, &body)
                ));
                std::thread::sleep(Duration::from_secs(3));
            }
            Err(ureq::Error::Transport(err)) if attempts_left > 0 => {
                warning(&format!("网络异常：{err}，2 秒后重试（接口幂等，可安全重试）"));
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(other) => return Err(api_error(other)),
        }
    }
}

fn confirm_publish() -> Result<bool> {
    print!("确认发布？(y/N) ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "YES"))
}

fn normalize_server_url(raw: &str) -> Result<String> {
    let url = url::Url::parse(raw.trim()).map_err(|_| {
        anyhow!("服务器地址必须使用 HTTPS；仅精确 localhost 或回环 IP 可使用 HTTP")
    })?;
    let loopback = match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };

    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(anyhow!(
            "服务器地址必须使用 HTTPS；仅精确 localhost 或回环 IP 可使用 HTTP"
        ));
    }

    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// 执行 `publish` 子命令。
pub fn run_publish(args: PublishArgs) -> Result<()> {
    let server = normalize_server_url(&args.server)?;

    step(1, 5, "解析插件");
    let path = match args.file {
        Some(path) => path,
        None => default_package_path(Path::new("."))?,
    };
    let package = parse_package(&path)?;
    let meta = &package.manifest.meta;
    detail(&format!("插件 ID：  {}", meta.id));
    detail(&format!("名称：     {}", meta.name));
    detail(&format!("版本：     {}", meta.version));
    detail(&format!("作者：     {}", meta.author));
    detail(&format!("类型：     {}", meta.kind));
    detail(&format!("协议版本： {}", meta.agreement_version));
    detail(&format!(
        "文件：     {}（{}）",
        package.path.display(),
        human_size(package.bytes.len() as u64)
    ));
    detail(&format!("SHA-256：  {}", package.sha256_hex));
    detail(&format!("服务器：   {server}"));
    if !args.yes && !confirm_publish()? {
        info("已取消发布");
        return Ok(());
    }

    step(2, 5, "创建上传会话");
    let agent = ureq::AgentBuilder::new().build();
    let token = login(&agent, &server)?;
    let session = create_session(&agent, &server, &token, &package)?;
    info(&format!("上传会话已创建：{}", session.upload_id));

    step(3, 5, "上传至 COS");
    upload_to_cos(&agent, &session, &package.bytes)?;
    info("文件已直传 COS（未经过网站服务器）");

    step(4, 5, "等待服务端校验");
    let outcome = complete_with_retry(&agent, &server, &token, &session.upload_id)?;
    if outcome.status != "published" {
        return Err(anyhow!("服务端返回异常状态：{}", outcome.status));
    }

    step(5, 5, "发布完成");
    info(&format!("插件 {} v{} 已发布", outcome.id, outcome.version));
    detail(&format!("下载地址：{}", outcome.download_url));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::{ZipWriter, write::SimpleFileOptions};

    /// `LocalPackage` 无 Debug，unwrap_err 不可用；统一走该辅助取错误文案。
    fn parse_err(path: &Path) -> String {
        match parse_package(path) {
            Ok(_) => panic!("{} 应解析失败", path.display()),
            Err(err) => err.to_string(),
        }
    }

    fn manifest_json(id: &str) -> String {
        format!(
            r#"{{
                "meta": {{
                    "id": "{id}", "name": "Demo", "author": "a", "description": "d",
                    "version": "1.0.0", "kind": "llm", "agreement-version": 1,
                    "url": "https://api.example.com"
                }},
                "models": [{{ "id": "m1" }}]
            }}"#
        )
    }

    fn build_fcplug(manifest: Option<&str>, with_wasm: bool) -> Vec<u8> {
        let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        if let Some(manifest) = manifest {
            writer.start_file("manifest.json", options).unwrap();
            writer.write_all(manifest.as_bytes()).unwrap();
        }
        if with_wasm {
            writer.start_file("plugin.wasm", options).unwrap();
            writer.write_all(b"wasm").unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fcplug-publish-test-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_package_computes_digests_and_validates_structure() {
        let dir = temp_dir("parse");
        let bytes = build_fcplug(Some(&manifest_json("demo")), true);
        let path = dir.join("demo.fcplug");
        std::fs::write(&path, &bytes).unwrap();

        let package = parse_package(&path).expect("合法包应解析成功");
        assert_eq!(package.manifest.meta.id, "demo");
        assert_eq!(package.sha256_hex, format!("{:x}", Sha256::digest(&bytes)));
        assert_eq!(
            package.md5_base64,
            base64::engine::general_purpose::STANDARD.encode(Md5::digest(&bytes))
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn parse_package_rejects_missing_parts() {
        let dir = temp_dir("missing");

        let no_manifest = dir.join("no-manifest.fcplug");
        std::fs::write(&no_manifest, build_fcplug(None, true)).unwrap();
        let err = parse_err(&no_manifest);
        assert!(err.contains("manifest.json"), "实际：{err}");

        let no_wasm = dir.join("no-wasm.fcplug");
        std::fs::write(&no_wasm, build_fcplug(Some(&manifest_json("demo")), false)).unwrap();
        let err = parse_err(&no_wasm);
        assert!(err.contains("plugin.wasm"), "实际：{err}");

        let not_zip = dir.join("not-zip.fcplug");
        std::fs::write(&not_zip, b"hello").unwrap();
        let err = parse_err(&not_zip);
        assert!(err.contains("ZIP"), "实际：{err}");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn default_package_path_reads_manifest_id() {
        let dir = temp_dir("default-path");
        std::fs::write(dir.join("manifest.json"), manifest_json("my-plugin")).unwrap();

        // dist 缺失：提示先 build
        let err = default_package_path(&dir).unwrap_err().to_string();
        assert!(err.contains("cargo fcplug build"), "实际：{err}");

        std::fs::create_dir_all(dir.join("dist")).unwrap();
        std::fs::write(
            dir.join("dist").join("my-plugin.fcplug"),
            build_fcplug(Some(&manifest_json("my-plugin")), true),
        )
        .unwrap();
        let path = default_package_path(&dir).expect("应定位到 dist 产物");
        assert!(path.ends_with(Path::new("dist").join("my-plugin.fcplug")));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn error_from_body_extracts_server_error_field() {
        let err = error_from_body(400, r#"{"error":"插件 demo 版本 1.0.0 已发布"}"#);
        assert!(err.to_string().contains("已发布"));
        let err = error_from_body(502, "Bad Gateway");
        assert!(err.to_string().contains("Bad Gateway"));
    }

    #[test]
    fn display_url_strips_query_signature() {
        let shown = display_url(
            "https://bucket.cos.ap-guangzhou.myqcloud.com/staging/plugin-uploads/u1.fcplug?q-signature=secret",
        );
        assert_eq!(
            shown,
            "https://bucket.cos.ap-guangzhou.myqcloud.com/staging/plugin-uploads/u1.fcplug"
        );
        assert!(!shown.contains("q-signature"));
    }

    #[test]
    fn server_url_allows_https_and_exact_loopback_hosts() {
        assert_eq!(
            normalize_server_url("https://plugins.example.com/").unwrap(),
            "https://plugins.example.com"
        );
        for server in [
            "http://localhost:3000",
            "http://127.0.0.1:3000",
            "http://[::1]:3000",
        ] {
            assert!(normalize_server_url(server).is_ok(), "应允许：{server}");
        }
    }

    #[test]
    fn server_url_rejects_insecure_non_loopback_hosts() {
        for server in [
            "http://localhost.evil.example",
            "http://127.0.0.1.evil.example",
            "http://192.168.1.10:3000",
            "ftp://plugins.example.com",
            "not-a-url",
        ] {
            let err = normalize_server_url(server).expect_err("不安全地址应该被拒绝");
            assert!(err.to_string().contains("HTTPS"), "{server}：{err}");
        }
    }
}
