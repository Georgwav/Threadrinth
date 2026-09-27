//! Installs a mod loader's server into a server folder and finds the Java
//! the server needs.

use super::{LaunchTarget, PluginPlatform};
use crate::state::{JavaVersion, ModLoader, State};
use crate::util::fetch::{fetch, fetch_json};
use crate::util::io::{self, IOError};
use reqwest::Method;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tokio::process::Command;
use tokio::sync::Mutex;

/// One Java install at a time: two servers starting together would
/// otherwise delete and extract the same Java folder over each other.
static JAVA_INSTALL: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// The Java a Minecraft version's server needs: an installed one the app
/// knows, or one it downloads (like for playing).
pub(super) async fn server_java(game_version: &str) -> crate::Result<PathBuf> {
    let state = State::get().await?;
    let (minecraft, index) =
        crate::launcher::resolve_minecraft_manifest(game_version, &state)
            .await?;
    let version_info = crate::launcher::download::download_version_info(
        &state,
        &minecraft.versions[index],
        None,
        None,
        None,
        None,
    )
    .await?;
    let major = version_info
        .java_version
        .as_ref()
        .map_or(8, |java| java.major_version);
    let _install = JAVA_INSTALL.lock().await;
    if let Some(java) = JavaVersion::get(major, &state.pool).await?
        && Path::new(&java.path).is_file()
    {
        return Ok(PathBuf::from(java.path));
    }
    // A download cut short, or a game install touching the same folder,
    // fails the extract; a fresh download fixes it.
    let path =
        match crate::api::jre::auto_install_java_with_loading(major, false)
            .await
        {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(
                    "Installing Java {major} failed, retrying: {error}"
                );
                crate::api::jre::auto_install_java_with_loading(major, false)
                    .await?
            }
        };
    let java = crate::api::jre::check_jre(path.clone()).await?;
    java.upsert(&state.pool).await?;
    Ok(path)
}

pub(super) fn java_command(java: &Path) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(java);
    #[cfg(windows)]
    {
        // No console window next to the app.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Downloads Paper's or Purpur's newest build for the Minecraft version.
pub(super) async fn download_plugin_platform(
    dir: &Path,
    platform: PluginPlatform,
    game_version: &str,
) -> crate::Result<LaunchTarget> {
    let state = State::get().await?;
    let unavailable = || {
        crate::Error::from(crate::ErrorKind::InputError(format!(
            "{} has no build for Minecraft {game_version} yet",
            match platform {
                PluginPlatform::Paper => "Paper",
                PluginPlatform::Purpur => "Purpur",
            }
        )))
    };
    let (url, file_name) = match platform {
        PluginPlatform::Paper => {
            let build: serde_json::Value = fetch_json(
                Method::GET,
                &format!(
                    "https://fill.papermc.io/v3/projects/paper/versions/{}/builds/latest",
                    urlencoding::encode(game_version)
                ),
                None,
                None,
                None,
                &state.fetch_semaphore,
                &state.pool,
            )
            .await
            .map_err(|_| unavailable())?;
            let url = build["downloads"]["server:default"]["url"]
                .as_str()
                .ok_or_else(unavailable)?
                .to_string();
            (url, "paper.jar")
        }
        PluginPlatform::Purpur => (
            format!(
                "https://api.purpurmc.org/v2/purpur/{}/latest/download",
                urlencoding::encode(game_version)
            ),
            "purpur.jar",
        ),
    };
    let jar =
        fetch(&url, None, None, None, &state.fetch_semaphore, &state.pool)
            .await
            .map_err(|_| unavailable())?;
    // A missing build answers with a small JSON error, not a jar.
    if !jar.starts_with(b"PK") {
        return Err(unavailable());
    }
    io::write(dir.join(file_name), &jar).await?;
    Ok(LaunchTarget::Jar {
        path: file_name.to_string(),
    })
}

/// Runs the loader's installer where one is needed (Quilt, Forge, NeoForge)
/// and returns how to start the server.
pub(super) async fn install_server(
    dir: &Path,
    game_version: &str,
    loader: ModLoader,
    loader_version: Option<&str>,
    launcher_file: &str,
) -> crate::Result<LaunchTarget> {
    match loader {
        ModLoader::Vanilla | ModLoader::Fabric => Ok(LaunchTarget::Jar {
            path: launcher_file.to_string(),
        }),
        ModLoader::Quilt => {
            let version = loader_version.unwrap_or_default();
            run_installer(
                dir,
                game_version,
                &[
                    "-jar",
                    launcher_file,
                    "install",
                    "server",
                    game_version,
                    version,
                    "--download-server",
                    "--install-dir=.",
                ],
            )
            .await?;
            Ok(LaunchTarget::Jar {
                path: "quilt-server-launch.jar".to_string(),
            })
        }
        ModLoader::Forge | ModLoader::NeoForge => {
            run_installer(
                dir,
                game_version,
                &["-jar", launcher_file, "--installServer"],
            )
            .await?;
            forge_launch_target(dir, launcher_file).await
        }
    }
}

async fn run_installer(
    dir: &Path,
    game_version: &str,
    args: &[&str],
) -> crate::Result<()> {
    let java = server_java(game_version).await?;
    let output = java_command(&java)
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|error| IOError::with_path(error, &java))?;
    let mut log = output.stdout;
    log.extend_from_slice(&output.stderr);
    io::write(dir.join("installer.log"), &log).await?;
    if !output.status.success() {
        let tail = String::from_utf8_lossy(&log)
            .lines()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(crate::ErrorKind::OtherError(format!(
            "The mod loader's server installer failed ({}):\n{tail}",
            output.status
        ))
        .into());
    }
    Ok(())
}

/// Modern Forge and NeoForge installers write `run.sh`, which names the
/// arguments file to start with; older Forge installs a runnable jar.
async fn forge_launch_target(
    dir: &Path,
    installer: &str,
) -> crate::Result<LaunchTarget> {
    if let Ok(script) = tokio::fs::read_to_string(dir.join("run.sh")).await
        && let Some(args) = args_file_in_script(&script)
    {
        return Ok(LaunchTarget::ArgsFile { path: args });
    }
    let mut entries = io::read_dir(dir).await?;
    let mut jars = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| IOError::with_path(error, dir))?
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".jar")
            && name != installer
            && !name.contains("installer")
            && (name.starts_with("forge") || name.starts_with("neoforge"))
        {
            jars.push(name);
        }
    }
    jars.sort();
    jars.pop()
        .map(|path| LaunchTarget::Jar { path })
        .ok_or_else(|| {
            crate::ErrorKind::OtherError(
                "The mod loader installed no server to start".to_string(),
            )
            .into()
        })
}

fn args_file_in_script(script: &str) -> Option<String> {
    script
        .split_whitespace()
        .filter_map(|word| word.trim_matches('"').strip_prefix('@'))
        .find(|path| path.ends_with("unix_args.txt"))
        .map(str::to_string)
}

/// The arguments file for this OS (Windows uses `win_args.txt`).
pub(super) fn platform_args_file(path: &str) -> String {
    if cfg!(windows) {
        path.replace("unix_args.txt", "win_args.txt")
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_forge_args_file() {
        let script = "#!/usr/bin/env sh\n# comment\njava @user_jvm_args.txt @libraries/net/minecraftforge/forge/1.20.1-47.2.0/unix_args.txt \"$@\"\n";
        assert_eq!(
            args_file_in_script(script).as_deref(),
            Some(
                "libraries/net/minecraftforge/forge/1.20.1-47.2.0/unix_args.txt"
            )
        );
        assert_eq!(args_file_in_script("java -jar server.jar"), None);
    }
}
