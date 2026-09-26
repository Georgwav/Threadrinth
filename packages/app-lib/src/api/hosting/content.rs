//! A server's mods or plugins: listing them (named from Modrinth where it
//! knows the file), turning them on and off, removing them, and installing
//! Modrinth projects with their required dependencies.

use super::{
    HostedServer, PluginPlatform, get_server, input, server_dir, write_server,
};
use crate::State;
use crate::state::ModLoader;
use crate::util::fetch::{fetch, fetch_json, sha1_async};
use crate::util::io::{self, IOError};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// How many levels of required dependencies an install follows.
const MAX_DEPENDENCY_DEPTH: usize = 3;
const DISABLED_SUFFIX: &str = ".disabled";

/// A mod or plugin file in the server.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ServerContent {
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
    /// The Modrinth project, when Modrinth knows the file.
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub icon_url: Option<String>,
    pub version_number: Option<String>,
}

/// A Modrinth project that runs on the server.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ServerSearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ServerSearchResults {
    pub hits: Vec<ServerSearchHit>,
    pub total: u64,
}

/// What installing a project added.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ServerInstall {
    /// File names added to the server.
    pub installed: Vec<String>,
    /// Required dependencies with no version for this server.
    pub missing_dependencies: Vec<String>,
    /// Plugins installed although they aren't marked for this Minecraft
    /// version (most plugins keep working on newer versions).
    pub unmarked: Vec<String>,
}

/// The folder the server loads content from: `plugins` on Paper and
/// Purpur, `mods` with a mod loader, none on vanilla.
pub(super) fn content_folder(server: &HostedServer) -> Option<&'static str> {
    if server.platform.is_some() {
        Some("plugins")
    } else if server.loader == ModLoader::Vanilla {
        None
    } else {
        Some("mods")
    }
}

/// The Modrinth loaders whose files run on the server.
pub(super) fn modrinth_loaders(server: &HostedServer) -> Vec<&'static str> {
    match server.platform {
        Some(PluginPlatform::Paper) => vec!["paper", "spigot", "bukkit"],
        Some(PluginPlatform::Purpur) => {
            vec!["purpur", "paper", "spigot", "bukkit"]
        }
        None => match server.loader {
            // Quilt also runs Fabric mods.
            ModLoader::Quilt => vec!["quilt", "fabric"],
            ModLoader::Fabric => vec!["fabric"],
            ModLoader::Forge => vec!["forge"],
            ModLoader::NeoForge => vec!["neoforge"],
            ModLoader::Vanilla => vec![],
        },
    }
}

async fn folder_path(server: &HostedServer) -> crate::Result<PathBuf> {
    let folder = content_folder(server).ok_or_else(|| {
        input("Vanilla servers can't load mods or plugins. Create a server with a mod loader, Paper or Purpur.")
    })?;
    Ok(server_dir(&server.id).await?.join(folder))
}

fn is_content_file(name: &str) -> bool {
    name.trim_end_matches(DISABLED_SUFFIX).ends_with(".jar")
}

/// The jar files in the server's mods or plugins folder, with names from
/// Modrinth.
#[tracing::instrument]
pub async fn server_content(id: &str) -> crate::Result<Vec<ServerContent>> {
    let server = get_server(id).await?;
    let Some(_) = content_folder(&server) else {
        return Ok(Vec::new());
    };
    let dir = folder_path(&server).await?;
    let mut files = Vec::new();
    if let Ok(mut entries) = io::read_dir(&dir).await {
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| IOError::with_path(error, &dir))?
        {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if !path.is_file() || !is_content_file(&file_name) {
                continue;
            }
            let bytes = io::read(&path).await?;
            let size = bytes.len() as u64;
            let sha1 = sha1_async(bytes.into()).await?;
            files.push((file_name, size, sha1));
        }
    }

    let hashes = files.iter().map(|(_, _, sha1)| sha1.clone()).collect();
    let known = identify(hashes).await.unwrap_or_else(|error| {
        tracing::warn!("Couldn't look up server content on Modrinth: {error}");
        HashMap::new()
    });
    let mut content = files
        .into_iter()
        .map(|(file_name, size, sha1)| {
            let project = known.get(&sha1);
            ServerContent {
                enabled: !file_name.ends_with(DISABLED_SUFFIX),
                file_name,
                size,
                project_id: project.map(|x| x.project_id.clone()),
                title: project.map(|x| x.title.clone()),
                icon_url: project.and_then(|x| x.icon_url.clone()),
                version_number: project.map(|x| x.version_number.clone()),
            }
        })
        .collect::<Vec<_>>();
    content.sort_by_key(|x| {
        x.title
            .clone()
            .unwrap_or_else(|| x.file_name.clone())
            .to_lowercase()
    });
    Ok(content)
}

/// The search facets for projects that run on the server: plugins for
/// Paper and Purpur, else server-side mods, for its Minecraft version.
pub(super) fn search_facets(server: &HostedServer) -> serde_json::Value {
    let loaders = modrinth_loaders(server)
        .into_iter()
        .map(|loader| format!("categories:{loader}"))
        .collect::<Vec<_>>();
    let project_type = if server.platform.is_some() {
        "project_type:plugin"
    } else {
        "project_type:mod"
    };
    let mut facets = vec![
        serde_json::json!([project_type]),
        serde_json::json!(loaders),
        serde_json::json!([format!("versions:{}", server.game_version)]),
    ];
    if server.platform.is_none() {
        facets.push(serde_json::json!([
            "server_side:required",
            "server_side:optional"
        ]));
    }
    serde_json::Value::Array(facets)
}

/// Searches Modrinth for projects that run on the server, most downloaded
/// first when there's no search text.
#[tracing::instrument]
pub async fn search_server_content(
    id: &str,
    query: &str,
    offset: u32,
) -> crate::Result<ServerSearchResults> {
    #[derive(Deserialize)]
    struct Hit {
        project_id: String,
        slug: String,
        title: String,
        description: String,
        author: String,
        icon_url: Option<String>,
        downloads: u64,
    }
    #[derive(Deserialize)]
    struct Results {
        hits: Vec<Hit>,
        total_hits: u64,
    }
    let server = get_server(id).await?;
    if content_folder(&server).is_none() {
        return Ok(ServerSearchResults {
            hits: Vec::new(),
            total: 0,
        });
    }
    let query = query.trim();
    let index = if query.is_empty() {
        "downloads"
    } else {
        "relevance"
    };
    let state = State::get().await?;
    let results: Results = fetch_json(
        Method::GET,
        &api(&format!(
            "search?query={}&facets={}&index={index}&offset={offset}&limit=20",
            urlencoding::encode(query),
            urlencoding::encode(&search_facets(&server).to_string())
        )),
        None,
        None,
        None,
        &state.api_semaphore,
        &state.pool,
    )
    .await?;
    Ok(ServerSearchResults {
        total: results.total_hits,
        hits: results
            .hits
            .into_iter()
            .map(|hit| ServerSearchHit {
                project_id: hit.project_id,
                slug: hit.slug,
                title: hit.title,
                description: hit.description,
                author: hit.author,
                icon_url: hit.icon_url.filter(|x| !x.is_empty()),
                downloads: hit.downloads,
            })
            .collect(),
    })
}

/// A server file's name, checked to stay in the content folder.
fn content_file(dir: &Path, file_name: &str) -> crate::Result<PathBuf> {
    if !super::is_plain_folder_name(file_name) || !is_content_file(file_name) {
        return Err(input(format!("Invalid file name: {file_name}")));
    }
    Ok(dir.join(file_name))
}

/// Turns a mod or plugin on or off (`.disabled` at the end of its name).
pub async fn set_server_content_enabled(
    id: &str,
    file_name: &str,
    enabled: bool,
) -> crate::Result<String> {
    let server = get_server(id).await?;
    let dir = folder_path(&server).await?;
    let path = content_file(&dir, file_name)?;
    let base = file_name.trim_end_matches(DISABLED_SUFFIX);
    let target = if enabled {
        base.to_string()
    } else {
        format!("{base}{DISABLED_SUFFIX}")
    };
    if target != file_name {
        io::rename_or_move(&path, &dir.join(&target)).await?;
    }
    Ok(target)
}

pub async fn remove_server_content(
    id: &str,
    file_name: &str,
) -> crate::Result<()> {
    let server = get_server(id).await?;
    let dir = folder_path(&server).await?;
    io::remove_file(content_file(&dir, file_name)?).await?;
    let base = file_name.trim_end_matches(DISABLED_SUFFIX);
    if server.added_content.iter().any(|x| x == base) {
        let mut server = server;
        server.added_content.retain(|x| x != base);
        write_server(&server).await?;
    }
    Ok(())
}

/// Installs the newest version of a Modrinth project (id or slug) for the
/// server's Minecraft version and loader, with its required dependencies.
#[tracing::instrument]
pub async fn install_server_project(
    id: &str,
    project: &str,
) -> crate::Result<ServerInstall> {
    let server = get_server(id).await?;
    let dir = folder_path(&server).await?;
    io::create_dir_all(&dir).await?;
    let loaders = modrinth_loaders(&server);

    // What the server already has, so dependencies aren't added twice.
    let present = server_content(id)
        .await?
        .into_iter()
        .filter_map(|x| x.project_id)
        .collect::<HashSet<_>>();

    let mut report = ServerInstall::default();
    let mut visited = HashSet::new();
    let mut queue = vec![(project.to_string(), 0_usize)];
    let mut first = true;
    while let Some((project, depth)) = queue.pop() {
        let info = get_project(&project).await?;
        if !visited.insert(info.id.clone()) {
            continue;
        }
        if first && info.server_side == "unsupported" {
            return Err(input(format!(
                "{} only works in the game, not on servers.",
                info.title
            )));
        }
        if !first && present.contains(&info.id) {
            continue;
        }
        let mut version =
            newest_version(&info.id, Some(&server.game_version), &loaders)
                .await?;
        if version.is_none() && server.platform.is_some() {
            version = newest_version(&info.id, None, &loaders).await?;
            if version.is_some() {
                report.unmarked.push(info.title.clone());
            }
        }
        let Some(version) = version else {
            if first {
                return Err(input(format!(
                    "{} has no version for Minecraft {} on this server.",
                    info.title, server.game_version
                )));
            }
            report.missing_dependencies.push(info.title);
            continue;
        };
        first = false;
        if depth < MAX_DEPENDENCY_DEPTH {
            for dependency in &version.dependencies {
                if dependency.dependency_type == "required"
                    && let Some(project_id) = &dependency.project_id
                {
                    queue.push((project_id.clone(), depth + 1));
                }
            }
        }
        let Some(file) = version
            .files
            .iter()
            .find(|file| file.primary)
            .or(version.files.first())
        else {
            continue;
        };
        let path = content_file(&dir, &file.filename)?;
        let state = State::get().await?;
        let bytes = fetch(
            &file.url,
            file.hashes.get("sha1").map(String::as_str),
            None,
            None,
            &state.fetch_semaphore,
            &state.pool,
        )
        .await?;
        io::write(&path, &bytes).await?;
        report.installed.push(file.filename.clone());
    }
    if !report.installed.is_empty() {
        // Re-read: nothing else changes the file meanwhile, but keep the
        // newest settings.
        let mut server = get_server(id).await?;
        for name in &report.installed {
            if !server.added_content.contains(name) {
                server.added_content.push(name.clone());
            }
        }
        write_server(&server).await?;
    }
    Ok(report)
}

#[derive(Deserialize)]
struct Project {
    id: String,
    title: String,
    #[serde(default)]
    server_side: String,
    icon_url: Option<String>,
}

#[derive(Deserialize)]
struct Version {
    project_id: String,
    version_number: String,
    #[serde(default)]
    files: Vec<VersionFile>,
    #[serde(default)]
    dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
struct VersionFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    hashes: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Dependency {
    project_id: Option<String>,
    dependency_type: String,
}

struct Known {
    project_id: String,
    title: String,
    icon_url: Option<String>,
    version_number: String,
}

fn api(path: &str) -> String {
    format!("{}{path}", env!("MODRINTH_API_URL"))
}

async fn get_project(id: &str) -> crate::Result<Project> {
    let state = State::get().await?;
    fetch_json(
        Method::GET,
        &api(&format!("project/{}", urlencoding::encode(id))),
        None,
        None,
        None,
        &state.api_semaphore,
        &state.pool,
    )
    .await
}

pub(super) fn version_query(
    game_version: Option<&str>,
    loaders: &[&str],
) -> String {
    let loaders = serde_json::to_string(loaders).unwrap_or_default();
    let mut query = format!("loaders={}", urlencoding::encode(&loaders));
    if let Some(game_version) = game_version {
        let game_versions =
            serde_json::to_string(&[game_version]).unwrap_or_default();
        query.push_str("&game_versions=");
        query.push_str(&urlencoding::encode(&game_versions));
    }
    query
}

/// The newest version for the Minecraft version (any, with `None`) and
/// loaders; releases win over betas and alphas.
async fn newest_version(
    project_id: &str,
    game_version: Option<&str>,
    loaders: &[&str],
) -> crate::Result<Option<Version>> {
    let state = State::get().await?;
    let versions: Vec<serde_json::Value> = fetch_json(
        Method::GET,
        &api(&format!(
            "project/{project_id}/version?{}",
            version_query(game_version, loaders)
        )),
        None,
        None,
        None,
        &state.api_semaphore,
        &state.pool,
    )
    .await?;
    // Newest first; the first release, else the newest of any kind.
    let chosen = versions
        .iter()
        .find(|x| x["version_type"] == "release")
        .or(versions.first())
        .cloned();
    Ok(chosen.map(serde_json::from_value).transpose()?)
}

/// Modrinth's projects for file hashes (SHA-1).
async fn identify(
    hashes: Vec<String>,
) -> crate::Result<HashMap<String, Known>> {
    if hashes.is_empty() {
        return Ok(HashMap::new());
    }
    let state = State::get().await?;
    let versions: HashMap<String, Version> = fetch_json(
        Method::POST,
        &api("version_files"),
        None,
        Some(serde_json::json!({ "hashes": hashes, "algorithm": "sha1" })),
        None,
        &state.api_semaphore,
        &state.pool,
    )
    .await?;
    let ids = versions
        .values()
        .map(|x| x.project_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let projects: Vec<Project> = fetch_json(
        Method::GET,
        &api(&format!(
            "projects?ids={}",
            urlencoding::encode(&serde_json::to_string(&ids)?)
        )),
        None,
        None,
        None,
        &state.api_semaphore,
        &state.pool,
    )
    .await?;
    let projects = projects
        .into_iter()
        .map(|x| (x.id.clone(), x))
        .collect::<HashMap<_, _>>();
    Ok(versions
        .into_iter()
        .filter_map(|(hash, version)| {
            let project = projects.get(&version.project_id)?;
            Some((
                hash,
                Known {
                    project_id: project.id.clone(),
                    title: project.title.clone(),
                    icon_url: project.icon_url.clone(),
                    version_number: version.version_number,
                },
            ))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_files_are_jars() {
        assert!(is_content_file("lithium.jar"));
        assert!(is_content_file("lithium.jar.disabled"));
        assert!(!is_content_file("config.toml"));
        let dir = Path::new("/servers/a/mods");
        assert!(content_file(dir, "../x.jar").is_err());
        assert!(content_file(dir, "a/b.jar").is_err());
        assert_eq!(
            content_file(dir, "lithium.jar").unwrap(),
            dir.join("lithium.jar")
        );
    }

    #[test]
    fn version_queries_are_encoded() {
        assert_eq!(
            version_query(Some("1.21.8"), &["paper", "spigot"]),
            "loaders=%5B%22paper%22%2C%22spigot%22%5D&game_versions=%5B%221.21.8%22%5D"
        );
        assert_eq!(
            version_query(None, &["fabric"]),
            "loaders=%5B%22fabric%22%5D"
        );
    }
}
