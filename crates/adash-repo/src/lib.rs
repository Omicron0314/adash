//! Read-only repository inspection for deployment planning.
//!
//! Only manifest files are read: package/dependency metadata, Dockerfiles and
//! compose descriptors. Source, `.env`, credentials and home-directory files
//! are never returned to the model. Local paths and public GitHub tarballs are
//! supported; the profile is a compact summary, not repository contents.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_FILE: usize = 256 * 1024;
const MAX_TOTAL: usize = 2 * 1024 * 1024;
const MAX_ARCHIVE: u64 = 20 * 1024 * 1024;
const MAX_DEPTH: usize = 4;
const MAX_LOCAL_FILES: usize = 3000;

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("仓库路径不存在：{0}")]
    Missing(String),
    #[error("仓库地址无效：{0}")]
    Source(String),
    #[error("仅支持 github.com 的公开仓库 URL 或本机目录")]
    Unsupported,
    #[error("下载仓库失败：{0}")]
    Network(String),
    #[error("仓库压缩包过大或格式无效")]
    Archive,
    #[error("读取仓库失败：{0}")]
    Io(String),
}
pub type Result<T> = std::result::Result<T, RepoError>;

/// One reason the repository may need a cloud service.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceHint {
    /// Product id (`uhost`, `eip`, `udb`, `redis`, `ulb`, `bucket`).
    pub id: String,
    /// Human-readable evidence from manifests, never file contents.
    pub reason: String,
    /// True when the app cannot run without it.
    pub required: bool,
}

/// Sanitized facts used by the GUI and the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoProfile {
    pub source: String,
    pub kind: String,
    pub name: String,
    pub frameworks: Vec<String>,
    pub languages: Vec<String>,
    pub services: Vec<ServiceHint>,
    pub manifests: Vec<String>,
    pub warnings: Vec<String>,
    pub summary: String,
}
impl RepoProfile {
    pub fn service_ids(&self) -> Vec<&str> {
        self.services
            .iter()
            .map(|service| service.id.as_str())
            .collect()
    }
}

/// Inspect a local directory or a public GitHub URL.
pub fn inspect_source(source: &str) -> Result<RepoProfile> {
    let source = source.trim();
    if source.is_empty() {
        return Err(RepoError::Source("请输入 GitHub URL 或本机仓库路径".into()));
    }
    if source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("git@")
        || source.starts_with("github.com/")
    {
        let github = GithubSource::parse(source)?;
        let files = github.download()?;
        return analyze(&github.url, "github", &files);
    }
    let path = local_path(source)?;
    let files = read_local(&path)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repository");
    let mut profile = analyze(&path.display().to_string(), "local", &files)?;
    profile.name = name.to_owned();
    Ok(profile)
}

/// The accepted GitHub URL shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubSource {
    pub owner: String,
    pub repo: String,
    pub branch: Option<String>,
    pub url: String,
}
impl GithubSource {
    pub fn parse(input: &str) -> Result<Self> {
        let normalized = if input.starts_with("git@github.com:") {
            format!(
                "https://github.com/{}",
                input.trim_start_matches("git@github.com:")
            )
        } else if input.starts_with("github.com/") {
            format!("https://{input}")
        } else {
            input.trim_end_matches('/').to_owned()
        };
        let url = url::Url::parse(&normalized).map_err(|_| RepoError::Source(input.into()))?;
        if url.scheme() != "https" || url.host_str() != Some("github.com") {
            return Err(RepoError::Unsupported);
        }
        if url.query().is_some()
            || url.fragment().is_some()
            || url.username() != ""
            || url.password().is_some()
        {
            return Err(RepoError::Source(
                "GitHub URL 不能包含凭据、查询参数或片段".into(),
            ));
        }
        let parts: Vec<_> = url
            .path_segments()
            .ok_or_else(|| RepoError::Source(input.into()))?
            .filter(|part| !part.is_empty())
            .collect();
        if parts.len() < 2 {
            return Err(RepoError::Source("GitHub URL 需要 owner/repo".into()));
        }
        let owner = parts[0].to_owned();
        let repo = parts[1].trim_end_matches(".git").to_owned();
        if !valid_name(&owner) || !valid_name(&repo) {
            return Err(RepoError::Source("GitHub owner/repo 含非法字符".into()));
        }
        let branch = if parts.len() >= 4 && parts[2] == "tree" {
            Some(parts[3..].join("/"))
        } else if parts.len() == 2 {
            None
        } else {
            return Err(RepoError::Source(
                "只支持仓库根或 /tree/<branch> 链接".into(),
            ));
        };
        let url = format!("https://github.com/{owner}/{repo}");
        Ok(Self {
            owner,
            repo,
            branch,
            url,
        })
    }
    fn download(&self) -> Result<BTreeMap<String, String>> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .connect_timeout(std::time::Duration::from_secs(8))
            .user_agent(concat!("adash/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| RepoError::Network(error.to_string()))?;
        let branches = self
            .branch
            .clone()
            .map_or_else(|| vec!["HEAD".to_owned()], |branch| vec![branch]);
        let mut last = String::new();
        for branch in branches {
            let url = if branch == "HEAD" {
                format!(
                    "https://codeload.github.com/{}/{}/tar.gz/HEAD",
                    self.owner, self.repo
                )
            } else {
                format!(
                    "https://codeload.github.com/{}/{}/tar.gz/refs/heads/{branch}",
                    self.owner, self.repo
                )
            };
            let response = match client.get(&url).send() {
                Ok(response) if response.status().is_success() => response,
                Ok(response) => {
                    last = format!("HTTP {}", response.status());
                    continue;
                }
                Err(error) => {
                    last = error.to_string();
                    continue;
                }
            };
            if response
                .content_length()
                .is_some_and(|length| length > MAX_ARCHIVE)
            {
                return Err(RepoError::Archive);
            }
            let mut bytes = Vec::new();
            response
                .take(MAX_ARCHIVE + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| RepoError::Network(error.to_string()))?;
            if bytes.len() as u64 > MAX_ARCHIVE {
                return Err(RepoError::Archive);
            }
            return read_archive(&bytes);
        }
        Err(RepoError::Network(if last.is_empty() {
            "无法定位默认分支".into()
        } else {
            last
        }))
    }
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn read_archive(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let mut files = BTreeMap::new();
    let mut total = 0usize;
    for entry in archive.entries().map_err(|_| RepoError::Archive)? {
        let mut entry = entry.map_err(|_| RepoError::Archive)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(|_| RepoError::Archive)?;
        let clean = path.components().skip(1).collect::<PathBuf>();
        if !is_manifest(&clean) {
            continue;
        }
        if entry.size() as usize > MAX_FILE {
            continue;
        }
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|_| RepoError::Archive)?;
        total += text.len();
        if total > MAX_TOTAL {
            break;
        }
        files.insert(clean.to_string_lossy().into_owned(), text);
    }
    if files.is_empty() {
        return Err(RepoError::Source("仓库中没有可识别的项目清单".into()));
    }
    Ok(files)
}

fn local_path(source: &str) -> Result<PathBuf> {
    let expanded = if let Some(rest) = source.strip_prefix("~/") {
        dirs::home_dir().unwrap_or_default().join(rest)
    } else {
        PathBuf::from(source)
    };
    let path = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .map_err(|error| RepoError::Io(error.to_string()))?
            .join(expanded)
    };
    let path = path
        .canonicalize()
        .map_err(|_| RepoError::Missing(source.into()))?;
    if !path.is_dir() {
        return Err(RepoError::Missing(source.into()));
    }
    Ok(path)
}

fn read_local(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    let mut total = 0usize;
    let mut seen = 0usize;
    walk(root, root, 0, &mut seen, &mut total, &mut files)?;
    if files.is_empty() {
        return Err(RepoError::Source("目录中没有可识别的项目清单".into()));
    }
    Ok(files)
}
fn walk(
    root: &Path,
    directory: &Path,
    depth: usize,
    seen: &mut usize,
    total: &mut usize,
    files: &mut BTreeMap<String, String>,
) -> Result<()> {
    if depth > MAX_DEPTH || *seen > MAX_LOCAL_FILES {
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .map_err(|error| RepoError::Io(error.to_string()))?
        .filter_map(|entry| entry.ok())
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        *seen += 1;
        if *seen > MAX_LOCAL_FILES {
            break;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            if !ignored_dir(&name) {
                walk(root, &path, depth + 1, seen, total, files)?;
            }
            continue;
        }
        if !is_manifest(&path) || metadata.len() as usize > MAX_FILE {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).map_err(|error| RepoError::Io(error.to_string()))?;
        *total += text.len();
        if *total > MAX_TOTAL {
            break;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        files.insert(relative, text);
    }
    Ok(())
}
fn ignored_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | "vendor"
            | ".venv"
            | "venv"
            | "__pycache__"
            | ".cache"
            | "coverage"
    )
}
fn is_manifest(path: &Path) -> bool {
    matches!(
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(""),
        "package.json"
            | "Cargo.toml"
            | "requirements.txt"
            | "pyproject.toml"
            | "Pipfile"
            | "go.mod"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "composer.json"
            | "Gemfile"
            | "Dockerfile"
            | "docker-compose.yml"
            | "docker-compose.yaml"
            | "compose.yml"
            | "compose.yaml"
            | "Procfile"
            | "next.config.js"
            | "next.config.ts"
    )
}

fn analyze(source: &str, kind: &str, files: &BTreeMap<String, String>) -> Result<RepoProfile> {
    let all = files
        .values()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    let mut frameworks = BTreeSet::new();
    let mut languages = BTreeSet::new();
    let mut deps = BTreeSet::new();
    let mut manifests = files.keys().cloned().collect::<Vec<_>>();
    manifests.sort();
    for (path, text) in files {
        let lower = text.to_lowercase();
        if path.ends_with("package.json") {
            languages.insert("Node.js".to_owned());
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                for key in ["dependencies", "devDependencies"] {
                    if let Some(map) = value.get(key).and_then(serde_json::Value::as_object) {
                        deps.extend(map.keys().map(|name| name.to_lowercase()));
                    }
                }
            }
        } else if path.ends_with("Cargo.toml") {
            languages.insert("Rust".to_owned());
            if lower.contains("axum") {
                frameworks.insert("Axum".to_owned());
            }
            if lower.contains("actix-web") {
                frameworks.insert("Actix".to_owned());
            }
            if lower.contains("rocket") {
                frameworks.insert("Rocket".to_owned());
            }
            if lower.contains("sqlx") || lower.contains("diesel") || lower.contains("sea-orm") {
                deps.insert("sql".into());
            }
            if lower.contains("redis") {
                deps.insert("redis".into());
            }
        } else if path.ends_with("requirements.txt")
            || path.ends_with("pyproject.toml")
            || path.ends_with("Pipfile")
        {
            languages.insert("Python".to_owned());
            for (needle, framework) in [
                ("fastapi", "FastAPI"),
                ("django", "Django"),
                ("flask", "Flask"),
                ("gunicorn", "Gunicorn"),
                ("uvicorn", "Uvicorn"),
            ] {
                if lower.contains(needle) {
                    frameworks.insert(framework.into());
                }
            }
            for needle in [
                "psycopg",
                "postgres",
                "mysql",
                "sqlalchemy",
                "pymongo",
                "redis",
                "celery",
                "boto3",
                "minio",
            ] {
                if lower.contains(needle) {
                    deps.insert(needle.into());
                }
            }
        } else if path.ends_with("go.mod") {
            languages.insert("Go".to_owned());
            for (needle, value) in [
                ("gin-gonic/gin", "Gin"),
                ("labstack/echo", "Echo"),
                ("gorm.io/gorm", "GORM"),
            ] {
                if lower.contains(needle) {
                    frameworks.insert(value.into());
                }
            }
            for needle in ["pgx", "mysql", "redis", "mongo", "minio"] {
                if lower.contains(needle) {
                    deps.insert(needle.into());
                }
            }
        } else if path.ends_with("pom.xml")
            || path.ends_with("build.gradle")
            || path.ends_with("build.gradle.kts")
        {
            languages.insert("Java".to_owned());
            if lower.contains("spring-boot") {
                frameworks.insert("Spring Boot".into());
            }
            for needle in ["mysql", "postgresql", "redis", "mongodb"] {
                if lower.contains(needle) {
                    deps.insert(needle.into());
                }
            }
        } else if path.ends_with("Dockerfile") {
            if lower.contains("node") {
                languages.insert("Node.js".to_owned());
            }
            if lower.contains("python") {
                languages.insert("Python".to_owned());
            }
            if lower.contains("nginx") {
                frameworks.insert("Nginx".into());
            }
        }
    }
    for dep in &deps {
        let dep = dep.as_str();
        if matches!(
            dep,
            "pg" | "postgres"
                | "postgresql"
                | "mysql"
                | "mysql2"
                | "psycopg"
                | "psycopg2"
                | "sqlalchemy"
                | "prisma"
                | "typeorm"
                | "sequelize"
                | "drizzle-orm"
                | "sqlx"
                | "diesel"
                | "sea-orm"
                | "pgx"
                | "gorm.io/gorm"
        ) {
            frameworks.insert("SQL".into());
        }
        if matches!(dep, "redis" | "ioredis" | "bullmq" | "bull" | "celery") {
            frameworks.insert("Redis".into());
        }
        if matches!(dep, "mongoose" | "mongodb" | "pymongo") {
            frameworks.insert("MongoDB".into());
        }
        if matches!(dep, "boto3" | "aws-sdk" | "minio" | "@aws-sdk/client-s3") {
            frameworks.insert("对象存储".into());
        }
    }
    for (needle, name) in [
        ("next", "Next.js"),
        ("nuxt", "Nuxt"),
        ("react", "React"),
        ("vue", "Vue"),
        ("svelte", "Svelte"),
        ("@angular/core", "Angular"),
        ("express", "Express"),
        ("fastify", "Fastify"),
        ("@nestjs/core", "NestJS"),
    ] {
        if deps.iter().any(|dep| dep == needle) {
            frameworks.insert(name.into());
        }
    }
    let web = !files.is_empty()
        || all.contains("http")
        || all.contains("listen")
        || all.contains("server");
    let sql = frameworks.contains("SQL");
    let redis = frameworks.contains("Redis");
    let object = frameworks.contains("对象存储");
    let mut services = Vec::new();
    services.push(ServiceHint {
        id: "uhost".into(),
        reason: "应用需要一台可运行的计算实例".into(),
        required: true,
    });
    if web || manifests.iter().any(|path| path.ends_with("Dockerfile")) {
        services.push(ServiceHint {
            id: "eip".into(),
            reason: "需要公网访问/部署入口".into(),
            required: true,
        });
    }
    if sql {
        services.push(ServiceHint {
            id: "udb".into(),
            reason: "清单包含 SQL/ORM 依赖；可选择托管数据库".into(),
            required: false,
        });
    }
    if redis {
        services.push(ServiceHint {
            id: "redis".into(),
            reason: "清单包含 Redis/队列/缓存依赖".into(),
            required: false,
        });
    }
    if object {
        services.push(ServiceHint {
            id: "bucket".into(),
            reason: "清单包含对象存储 SDK".into(),
            required: false,
        });
    }
    if frameworks.contains("Nginx") {
        services.push(ServiceHint {
            id: "ulb".into(),
            reason: "容器镜像包含 Nginx，可考虑负载均衡".into(),
            required: false,
        });
    }
    let frameworks: Vec<_> = frameworks.into_iter().collect();
    let languages: Vec<_> = languages.into_iter().collect();
    let mut warnings = Vec::new();
    if files.is_empty() {
        warnings.push("没有找到支持的清单，计划将按最小单机服务估算".into());
    }
    let language_summary = if languages.is_empty() {
        "未知语言".to_owned()
    } else {
        languages.join(" / ")
    };
    let framework_summary = if frameworks.is_empty() {
        "未识别框架".to_owned()
    } else {
        frameworks.join(" / ")
    };
    let service_summary = if services.len() > 1 {
        services
            .iter()
            .skip(1)
            .map(|service| service.id.as_str())
            .collect::<Vec<_>>()
            .join(" + ")
    } else {
        "单机".to_owned()
    };
    let summary = format!(
        "{language_summary} · {framework_summary} · {service_summary} · {} 个仓库清单",
        manifests.len()
    );
    let name = source
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("repository")
        .trim_end_matches(".git")
        .to_owned();
    Ok(RepoProfile {
        source: source.to_owned(),
        kind: kind.into(),
        name,
        frameworks,
        languages,
        services,
        manifests,
        warnings,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn github_urls_preserve_nested_branches_and_reject_credentials() {
        let source = GithubSource::parse("https://github.com/acme/app/tree/release/v1").unwrap();
        assert_eq!(source.owner, "acme");
        assert_eq!(source.repo, "app");
        assert_eq!(source.branch.as_deref(), Some("release/v1"));
        assert!(GithubSource::parse("https://user:pass@github.com/acme/app").is_err());
        assert!(GithubSource::parse("https://gitlab.com/acme/app").is_err());
    }
    #[test]
    fn local_manifests_infer_services_without_reading_env_files() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"dependencies":{"next":"15","pg":"8","ioredis":"5","@aws-sdk/client-s3":"3"}}"#,
        )
        .unwrap();
        std::fs::write(
            directory.path().join(".env"),
            "DATABASE_URL=postgres://user:secret@example/db",
        )
        .unwrap();
        let profile = inspect_source(directory.path().to_str().unwrap()).unwrap();
        assert!(
            profile
                .frameworks
                .iter()
                .any(|framework| framework == "Next.js")
        );
        assert!(profile.service_ids().contains(&"udb"));
        assert!(profile.service_ids().contains(&"redis"));
        assert!(profile.service_ids().contains(&"bucket"));
        assert!(!serde_json::to_string(&profile).unwrap().contains("secret"));
        assert!(!profile.manifests.iter().any(|path| path.ends_with(".env")));
    }
    #[test]
    fn a_static_checkout_needs_host_and_public_ip() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"devDependencies":{"vite":"6"}}"#,
        )
        .unwrap();
        std::fs::write(directory.path().join("index.html"), "<html></html>").unwrap();
        let profile = inspect_source(directory.path().to_str().unwrap()).unwrap();
        assert!(profile.service_ids().contains(&"uhost"));
        assert!(profile.service_ids().contains(&"eip"));
        assert!(!profile.service_ids().contains(&"udb"));
    }
}
