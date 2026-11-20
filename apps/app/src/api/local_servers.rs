//! Modrinth Next: local Minecraft servers.
//!
//! Lets the user create Minecraft servers on their own computer (Vanilla,
//! Fabric, Paper, Forge, NeoForge), start/stop them, use the console and
//! install mods/plugins from Modrinth (including required dependencies).
//!
//! Servers live in `<config dir>/local-servers/<id>/`. Each server folder
//! holds a `modrinth-next-server.json` with its metadata, so no database
//! migration is needed.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, anyhow, bail};
use serde::{Deserialize, Serialize};
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{Mutex, oneshot};

use crate::api::Result;
use theseus::prelude::*;

const META_FILE: &str = "modrinth-next-server.json";
const MAX_LOG_LINES: usize = 2000;
const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const MOJANG_MANIFEST: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const PAPER_FILL: &str = "https://fill.papermc.io/v3/projects/paper";
const FORGE_MAVEN: &str =
    "https://maven.minecraftforge.net/net/minecraftforge/forge";
const NEOFORGE_MAVEN: &str =
    "https://maven.neoforged.net/releases/net/neoforged/neoforge";

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::new("local-servers")
        .setup(|app, _api| {
            app.manage(LocalServersState::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            local_servers_list,
            local_servers_get,
            local_servers_create,
            local_servers_delete,
            local_servers_start,
            local_servers_stop,
            local_servers_kill,
            local_servers_send_command,
            local_servers_get_logs,
            local_servers_get_properties,
            local_servers_set_properties,
            local_servers_update_settings,
            local_servers_get_game_versions,
            local_servers_get_loader_versions,
            local_servers_list_content,
            local_servers_toggle_content,
            local_servers_remove_content,
            local_servers_search_content,
            local_servers_install_project,
            local_servers_get_network_info,
        ])
        .build()
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServerLoader {
    Vanilla,
    Fabric,
    Paper,
    Forge,
    Neoforge,
}

impl ServerLoader {
    /// Modrinth loader names that are compatible with this server software.
    fn modrinth_loaders(self) -> &'static [&'static str] {
        match self {
            ServerLoader::Vanilla => &["datapack"],
            ServerLoader::Fabric => &["fabric"],
            ServerLoader::Paper => &["paper", "spigot", "bukkit"],
            ServerLoader::Forge => &["forge"],
            ServerLoader::Neoforge => &["neoforge"],
        }
    }

    fn content_dir(self) -> &'static str {
        match self {
            ServerLoader::Vanilla => "world/datapacks",
            ServerLoader::Paper => "plugins",
            _ => "mods",
        }
    }

    fn project_type(self) -> &'static str {
        match self {
            ServerLoader::Vanilla => "datapack",
            ServerLoader::Paper => "plugin",
            _ => "mod",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "kind", content = "path", rename_all = "snake_case")]
pub enum LaunchTarget {
    /// `java -jar <path>`
    Jar(String),
    /// `java @<path>` (modern Forge / NeoForge argument files)
    ArgFile(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InstalledProject {
    pub project_id: String,
    pub version_id: String,
    pub file_name: String,
    pub title: String,
    pub icon_url: Option<String>,
    pub version_number: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LocalServerMeta {
    pub id: String,
    pub name: String,
    pub loader: ServerLoader,
    pub game_version: String,
    pub loader_version: Option<String>,
    pub memory_mb: u32,
    pub java_major: u32,
    #[serde(default)]
    pub jvm_args: Vec<String>,
    pub launch: LaunchTarget,
    pub created: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub projects: Vec<InstalledProject>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServerStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
}

#[derive(Serialize, Debug, Clone)]
pub struct LocalServerInfo {
    #[serde(flatten)]
    pub meta: LocalServerMeta,
    pub path: String,
    pub status: ServerStatus,
    pub port: u16,
}

#[derive(Deserialize, Debug)]
pub struct CreateLocalServer {
    pub name: String,
    pub loader: ServerLoader,
    pub game_version: String,
    pub loader_version: Option<String>,
    pub memory_mb: u32,
    pub port: u16,
    pub accept_eula: bool,
    #[serde(default)]
    pub motd: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct UpdateLocalServerSettings {
    pub name: Option<String>,
    pub memory_mb: Option<u32>,
    pub jvm_args: Option<Vec<String>>,
}

#[derive(Serialize, Debug, Clone)]
pub struct ContentFile {
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
    pub project: Option<InstalledProject>,
}

#[derive(Serialize, Debug, Clone)]
pub struct NetworkInfo {
    pub port: u16,
    pub local_address: String,
    pub lan_address: Option<String>,
}

#[derive(Serialize, Clone)]
struct LogEvent<'a> {
    id: &'a str,
    line: &'a str,
}

#[derive(Serialize, Clone)]
struct StatusEvent<'a> {
    id: &'a str,
    status: ServerStatus,
}

#[derive(Serialize, Clone)]
struct ProgressEvent<'a> {
    id: &'a str,
    message: &'a str,
}

struct RunningServer {
    status: ServerStatus,
    stdin: Option<ChildStdin>,
    kill: Option<oneshot::Sender<()>>,
}

#[derive(Default)]
pub struct LocalServersState {
    running: Mutex<HashMap<String, RunningServer>>,
    logs: Mutex<HashMap<String, VecDeque<String>>>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn servers_root() -> anyhow::Result<PathBuf> {
    let state = State::get().await?;
    let root = state.directories.config_dir.join("local-servers");
    tokio::fs::create_dir_all(&root).await?;
    Ok(root)
}

fn validate_id(id: &str) -> anyhow::Result<()> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("Invalid server id");
    }
    Ok(())
}

/// Only allows plain file names (no path separators, no `..`).
fn validate_file_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty()
        || name.contains(['/', '\\'])
        || name == "."
        || name == ".."
        || name.contains("..")
    {
        bail!("Invalid file name");
    }
    Ok(())
}

async fn server_dir(id: &str) -> anyhow::Result<PathBuf> {
    validate_id(id)?;
    Ok(servers_root().await?.join(id))
}

async fn read_meta(dir: &Path) -> anyhow::Result<LocalServerMeta> {
    let raw = tokio::fs::read(dir.join(META_FILE))
        .await
        .with_context(|| format!("Reading {}", dir.display()))?;
    Ok(serde_json::from_slice(&raw)?)
}

async fn write_meta(dir: &Path, meta: &LocalServerMeta) -> anyhow::Result<()> {
    let raw = serde_json::to_vec_pretty(meta)?;
    let tmp = dir.join(format!("{META_FILE}.tmp"));
    tokio::fs::write(&tmp, raw).await?;
    tokio::fs::rename(&tmp, dir.join(META_FILE)).await?;
    Ok(())
}

fn http() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(theseus::launcher_user_agent())
        .build()?)
}

async fn get_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<T> {
    let res = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Request to {url} failed"))?;
    if !res.status().is_success() {
        bail!("Request to {url} failed with status {}", res.status());
    }
    Ok(res.json::<T>().await?)
}

async fn get_text(
    client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<String> {
    let res = client.get(url).send().await?;
    if !res.status().is_success() {
        bail!("Request to {url} failed with status {}", res.status());
    }
    Ok(res.text().await?)
}

async fn download_to(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
) -> anyhow::Result<()> {
    let res = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Downloading {url}"))?;
    if !res.status().is_success() {
        bail!("Downloading {url} failed with status {}", res.status());
    }
    let bytes = res.bytes().await?;
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = dest.with_extension("part");
    tokio::fs::write(&tmp, &bytes).await?;
    tokio::fs::rename(&tmp, dest).await?;
    Ok(())
}

/// Extracts all `<version>` entries from a maven-metadata.xml.
fn maven_versions(xml: &str) -> Vec<String> {
    xml.split("<version>")
        .skip(1)
        .filter_map(|part| part.split("</version>").next())
        .map(|v| v.trim().to_string())
        .collect()
}

/// Maps a Forge version (`1.20.1-47.3.0`) to its Minecraft version.
fn forge_game_version(v: &str) -> Option<String> {
    v.split('-').next().map(str::to_string)
}

/// Maps a NeoForge version (`21.1.77`, `20.4.237`, `26.1.0.5-beta`) to the
/// Minecraft version it targets.
fn neoforge_game_version(v: &str) -> Option<String> {
    let base = v.split('-').next()?;
    let parts: Vec<&str> = base.split('.').collect();
    let major: u32 = parts.first()?.parse().ok()?;
    let minor: u32 = parts.get(1)?.parse().ok()?;
    if major >= 25 {
        // Year based Minecraft versions (e.g. 26.1, 26.1.1)
        let patch = if parts.len() >= 4 { parts.get(2) } else { None };
        match patch {
            Some(p) if *p != "0" => Some(format!("{major}.{minor}.{p}")),
            _ => Some(format!("{major}.{minor}")),
        }
    } else if minor == 0 {
        Some(format!("1.{major}"))
    } else {
        Some(format!("1.{major}.{minor}"))
    }
}

#[derive(Deserialize)]
struct MojangManifest {
    versions: Vec<MojangManifestVersion>,
}

#[derive(Deserialize)]
struct MojangManifestVersion {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

#[derive(Deserialize)]
struct MojangVersion {
    #[serde(rename = "javaVersion")]
    java_version: Option<MojangJavaVersion>,
    downloads: MojangDownloads,
}

#[derive(Deserialize)]
struct MojangJavaVersion {
    #[serde(rename = "majorVersion")]
    major_version: u32,
}

#[derive(Deserialize)]
struct MojangDownloads {
    server: Option<MojangDownload>,
}

#[derive(Deserialize)]
struct MojangDownload {
    url: String,
}

async fn mojang_version(
    client: &reqwest::Client,
    game_version: &str,
) -> anyhow::Result<MojangVersion> {
    let manifest: MojangManifest = get_json(client, MOJANG_MANIFEST).await?;
    let entry = manifest
        .versions
        .into_iter()
        .find(|v| v.id == game_version)
        .ok_or_else(|| anyhow!("Unknown Minecraft version {game_version}"))?;
    get_json(client, &entry.url).await
}

/// Finds a Java executable for the given major version, downloading one
/// through the regular Modrinth App Java management if necessary.
async fn resolve_java(major: u32) -> anyhow::Result<PathBuf> {
    // Copy the path out so no DashMap guard is held across an await.
    let configured: Option<PathBuf> = jre::get_java_versions()
        .await?
        .get(&major)
        .map(|java| PathBuf::from(&java.path))
        .filter(|p| p.is_file());
    let mut path = match configured {
        Some(p) => p,
        None => jre::auto_install_java_with_loading(major, false).await?,
    };

    // Prefer the console executable so stdin/stdout work properly.
    #[cfg(windows)]
    if path
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("javaw.exe"))
    {
        let console = path.with_file_name("java.exe");
        if console.is_file() {
            path = console;
        }
    }
    #[cfg(not(windows))]
    {
        let _ = &mut path;
    }

    Ok(path)
}

fn java_command(java: &Path) -> Command {
    let mut cmd = Command::new(java);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: don't open a console window for the server
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn emit_progress<R: Runtime>(app: &AppHandle<R>, id: &str, message: &str) {
    let _ = app.emit("local-server://progress", ProgressEvent { id, message });
}

async fn read_properties(dir: &Path) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    if let Ok(raw) =
        tokio::fs::read_to_string(dir.join("server.properties")).await
    {
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('!')
            {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    map
}

async fn write_properties(
    dir: &Path,
    props: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let mut out = String::from(
        "#Minecraft server properties\n#Edited with Modrinth Next\n",
    );
    for (k, v) in props {
        if k.contains(['\n', '\r', '=']) || v.contains(['\n', '\r']) {
            bail!("Invalid server.properties entry: {k}");
        }
        out.push_str(k);
        out.push('=');
        out.push_str(v);
        out.push('\n');
    }
    tokio::fs::write(dir.join("server.properties"), out).await?;
    Ok(())
}

fn port_of(props: &BTreeMap<String, String>) -> u16 {
    props
        .get("server-port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(25565)
}

async fn status_of(state: &LocalServersState, id: &str) -> ServerStatus {
    state
        .running
        .lock()
        .await
        .get(id)
        .map(|s| s.status)
        .unwrap_or(ServerStatus::Stopped)
}

async fn info_for(
    state: &LocalServersState,
    dir: &Path,
) -> anyhow::Result<LocalServerInfo> {
    let meta = read_meta(dir).await?;
    let props = read_properties(dir).await;
    let status = status_of(state, &meta.id).await;
    Ok(LocalServerInfo {
        path: dir.display().to_string(),
        port: port_of(&props),
        status,
        meta,
    })
}

async fn set_status<R: Runtime>(
    app: &AppHandle<R>,
    state: &LocalServersState,
    id: &str,
    status: ServerStatus,
) {
    {
        let mut running = state.running.lock().await;
        if status == ServerStatus::Stopped {
            running.remove(id);
        } else if let Some(s) = running.get_mut(id) {
            s.status = status;
        }
    }
    let _ = app.emit("local-server://status", StatusEvent { id, status });
}

async fn push_log<R: Runtime>(
    app: &AppHandle<R>,
    state: &LocalServersState,
    id: &str,
    line: &str,
) {
    {
        let mut logs = state.logs.lock().await;
        let buf = logs.entry(id.to_string()).or_default();
        buf.push_back(line.to_string());
        while buf.len() > MAX_LOG_LINES {
            buf.pop_front();
        }
    }
    let _ = app.emit("local-server://log", LogEvent { id, line });
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

async fn install_server_software<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    dir: &Path,
    req: &CreateLocalServer,
) -> anyhow::Result<(LaunchTarget, u32, Option<String>)> {
    let client = http()?;

    emit_progress(app, id, "Minecraft-Version wird geladen…");
    let vanilla = mojang_version(&client, &req.game_version).await?;
    let java_major = vanilla
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or(8);

    match req.loader {
        ServerLoader::Vanilla => {
            let server = vanilla.downloads.server.ok_or_else(|| {
                anyhow!("Für diese Version gibt es keinen offiziellen Server")
            })?;
            emit_progress(app, id, "Server wird heruntergeladen…");
            download_to(&client, &server.url, &dir.join("server.jar")).await?;
            Ok((LaunchTarget::Jar("server.jar".into()), java_major, None))
        }
        ServerLoader::Fabric => {
            #[derive(Deserialize)]
            struct FabricVersion {
                version: String,
                stable: bool,
            }
            let loader = match &req.loader_version {
                Some(v) if !v.is_empty() => v.clone(),
                _ => {
                    let loaders: Vec<FabricVersion> = get_json(
                        &client,
                        &format!("{FABRIC_META}/versions/loader"),
                    )
                    .await?;
                    loaders
                        .into_iter()
                        .find(|l| l.stable)
                        .map(|l| l.version)
                        .ok_or_else(|| anyhow!("No Fabric loader found"))?
                }
            };
            let installers: Vec<FabricVersion> =
                get_json(&client, &format!("{FABRIC_META}/versions/installer"))
                    .await?;
            let installer = installers
                .into_iter()
                .find(|i| i.stable)
                .map(|i| i.version)
                .ok_or_else(|| anyhow!("No Fabric installer found"))?;
            emit_progress(app, id, "Fabric-Server wird heruntergeladen…");
            let url = format!(
                "{FABRIC_META}/versions/loader/{}/{}/{}/server/jar",
                req.game_version, loader, installer
            );
            download_to(&client, &url, &dir.join("server.jar")).await?;
            Ok((
                LaunchTarget::Jar("server.jar".into()),
                java_major,
                Some(loader),
            ))
        }
        ServerLoader::Paper => {
            #[derive(Deserialize)]
            struct PaperBuild {
                id: u64,
                downloads: HashMap<String, PaperDownload>,
            }
            #[derive(Deserialize)]
            struct PaperDownload {
                url: String,
            }
            emit_progress(app, id, "Paper wird heruntergeladen…");
            let build: PaperBuild = get_json(
                &client,
                &format!(
                    "{PAPER_FILL}/versions/{}/builds/latest",
                    req.game_version
                ),
            )
            .await?;
            let download = build
                .downloads
                .get("server:default")
                .ok_or_else(|| anyhow!("Paper build has no server download"))?;
            download_to(&client, &download.url, &dir.join("server.jar"))
                .await?;
            Ok((
                LaunchTarget::Jar("server.jar".into()),
                java_major,
                Some(build.id.to_string()),
            ))
        }
        ServerLoader::Forge | ServerLoader::Neoforge => {
            let is_forge = req.loader == ServerLoader::Forge;
            let loader_version = match &req.loader_version {
                Some(v) if !v.is_empty() => v.clone(),
                _ => latest_loader_version(
                    &client,
                    req.loader,
                    &req.game_version,
                )
                .await?
                .ok_or_else(|| {
                    anyhow!(
                        "Keine Loader-Version für {} gefunden",
                        req.game_version
                    )
                })?,
            };
            let installer_url = if is_forge {
                format!(
                    "{FORGE_MAVEN}/{loader_version}/forge-{loader_version}-installer.jar"
                )
            } else {
                format!(
                    "{NEOFORGE_MAVEN}/{loader_version}/neoforge-{loader_version}-installer.jar"
                )
            };
            emit_progress(app, id, "Installer wird heruntergeladen…");
            let installer = dir.join("installer.jar");
            download_to(&client, &installer_url, &installer).await?;

            emit_progress(
                app,
                id,
                &format!("Java {java_major} wird vorbereitet…"),
            );
            let java = resolve_java(java_major).await?;

            emit_progress(
                app,
                id,
                "Server wird installiert (kann etwas dauern)…",
            );
            let output = java_command(&java)
                .current_dir(dir)
                .arg("-jar")
                .arg("installer.jar")
                .arg("--installServer")
                .stdin(Stdio::null())
                .output()
                .await
                .context("Installer konnte nicht gestartet werden")?;
            let _ = tokio::fs::remove_file(&installer).await;
            let _ = tokio::fs::remove_file(dir.join("installer.jar.log")).await;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let tail: String = stdout
                    .lines()
                    .chain(stderr.lines())
                    .rev()
                    .take(10)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                bail!("Installation fehlgeschlagen:\n{tail}");
            }

            let launch =
                find_forge_launch_target(dir, is_forge, &loader_version)
                    .await?;
            Ok((launch, java_major, Some(loader_version)))
        }
    }
}

async fn find_forge_launch_target(
    dir: &Path,
    is_forge: bool,
    loader_version: &str,
) -> anyhow::Result<LaunchTarget> {
    let args_file = if cfg!(windows) {
        "win_args.txt"
    } else {
        "unix_args.txt"
    };
    let lib_dir = if is_forge {
        format!("libraries/net/minecraftforge/forge/{loader_version}")
    } else {
        format!("libraries/net/neoforged/neoforge/{loader_version}")
    };
    let rel = format!("{lib_dir}/{args_file}");
    if dir.join(&rel).is_file() {
        return Ok(LaunchTarget::ArgFile(rel));
    }

    // Legacy Forge (< 1.17) ships a runnable jar in the server root.
    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("forge-")
            && name.ends_with(".jar")
            && !name.contains("installer")
        {
            return Ok(LaunchTarget::Jar(name));
        }
    }
    bail!("Startdatei des Servers wurde nach der Installation nicht gefunden")
}

async fn latest_loader_version(
    client: &reqwest::Client,
    loader: ServerLoader,
    game_version: &str,
) -> anyhow::Result<Option<String>> {
    Ok(loader_versions(client, loader, game_version)
        .await?
        .into_iter()
        .next())
}

/// Loader versions for the given Minecraft version, newest first.
async fn loader_versions(
    client: &reqwest::Client,
    loader: ServerLoader,
    game_version: &str,
) -> anyhow::Result<Vec<String>> {
    match loader {
        ServerLoader::Vanilla | ServerLoader::Paper => Ok(vec![]),
        ServerLoader::Fabric => {
            #[derive(Deserialize)]
            struct Entry {
                loader: FabricLoader,
            }
            #[derive(Deserialize)]
            struct FabricLoader {
                version: String,
            }
            let list: Vec<Entry> = get_json(
                client,
                &format!("{FABRIC_META}/versions/loader/{game_version}"),
            )
            .await?;
            Ok(list.into_iter().map(|e| e.loader.version).collect())
        }
        ServerLoader::Forge => {
            let xml =
                get_text(client, &format!("{FORGE_MAVEN}/maven-metadata.xml"))
                    .await?;
            let prefix = format!("{game_version}-");
            let mut list: Vec<String> = maven_versions(&xml)
                .into_iter()
                .filter(|v| v.starts_with(&prefix))
                .collect();
            list.reverse();
            Ok(list)
        }
        ServerLoader::Neoforge => {
            let xml = get_text(
                client,
                &format!("{NEOFORGE_MAVEN}/maven-metadata.xml"),
            )
            .await?;
            let mut list: Vec<String> = maven_versions(&xml)
                .into_iter()
                .filter(|v| {
                    neoforge_game_version(v).as_deref() == Some(game_version)
                })
                .collect();
            list.reverse();
            Ok(list)
        }
    }
}

// ---------------------------------------------------------------------------
// Commands: servers
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn local_servers_list<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<LocalServerInfo>> {
    let state = app.state::<LocalServersState>();
    let root = servers_root().await?;
    let mut out = Vec::new();
    let mut entries = tokio::fs::read_dir(&root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if !path.join(META_FILE).is_file() {
            continue;
        }
        match info_for(&state, &path).await {
            Ok(info) => out.push(info),
            Err(e) => tracing::warn!(
                "Skipping local server {}: {e:#}",
                path.display()
            ),
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.meta.created));
    Ok(out)
}

#[tauri::command]
pub async fn local_servers_get<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<LocalServerInfo> {
    let state = app.state::<LocalServersState>();
    let dir = server_dir(&id).await?;
    Ok(info_for(&state, &dir).await?)
}

#[tauri::command]
pub async fn local_servers_create<R: Runtime>(
    app: AppHandle<R>,
    request: CreateLocalServer,
) -> Result<LocalServerInfo> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(anyhow!("Bitte gib einen Namen ein").into());
    }
    if !request.accept_eula {
        return Err(anyhow!(
            "Du musst die Minecraft-EULA akzeptieren, um einen Server zu erstellen"
        )
        .into());
    }

    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    let dir = server_dir(&id).await?;
    tokio::fs::create_dir_all(&dir).await?;

    let result = async {
        let (launch, java_major, loader_version) =
            install_server_software(&app, &id, &dir, &request).await?;

        emit_progress(&app, &id, &format!("Java {java_major} wird vorbereitet…"));
        resolve_java(java_major).await?;

        tokio::fs::write(
            dir.join("eula.txt"),
            "# Accepted in Modrinth Next (https://aka.ms/MinecraftEULA)\neula=true\n",
        )
        .await?;

        let mut props = BTreeMap::new();
        props.insert("server-port".to_string(), request.port.to_string());
        props.insert(
            "motd".to_string(),
            request
                .motd
                .clone()
                .filter(|m| !m.trim().is_empty())
                .unwrap_or_else(|| name.to_string()),
        );
        props.insert("enable-command-block".into(), "false".into());
        props.insert("max-players".into(), "20".into());
        props.insert("online-mode".into(), "true".into());
        props.insert("difficulty".into(), "easy".into());
        props.insert("gamemode".into(), "survival".into());
        props.insert("pvp".into(), "true".into());
        write_properties(&dir, &props).await?;

        let meta = LocalServerMeta {
            id: id.clone(),
            name: name.to_string(),
            loader: request.loader,
            game_version: request.game_version.clone(),
            loader_version,
            memory_mb: request.memory_mb.clamp(512, 65536),
            java_major,
            jvm_args: vec![],
            launch,
            created: chrono::Utc::now(),
            projects: vec![],
        };
        write_meta(&dir, &meta).await?;
        emit_progress(&app, &id, "Fertig!");
        anyhow::Ok(())
    }
    .await;

    if let Err(e) = result {
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return Err(e.into());
    }

    let state = app.state::<LocalServersState>();
    Ok(info_for(&state, &dir).await?)
}

#[tauri::command]
pub async fn local_servers_delete<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<()> {
    let state = app.state::<LocalServersState>();
    if status_of(&state, &id).await != ServerStatus::Stopped {
        return Err(anyhow!("Stoppe den Server, bevor du ihn löschst").into());
    }
    let dir = server_dir(&id).await?;
    if dir.join(META_FILE).is_file() {
        tokio::fs::remove_dir_all(&dir).await?;
    }
    state.logs.lock().await.remove(&id);
    Ok(())
}

#[tauri::command]
pub async fn local_servers_update_settings<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    settings: UpdateLocalServerSettings,
) -> Result<LocalServerInfo> {
    let state = app.state::<LocalServersState>();
    let dir = server_dir(&id).await?;
    let mut meta = read_meta(&dir).await?;
    if let Some(name) = settings.name {
        let name = name.trim();
        if !name.is_empty() {
            meta.name = name.to_string();
        }
    }
    if let Some(mem) = settings.memory_mb {
        meta.memory_mb = mem.clamp(512, 65536);
    }
    if let Some(args) = settings.jvm_args {
        meta.jvm_args =
            args.into_iter().filter(|a| !a.trim().is_empty()).collect();
    }
    write_meta(&dir, &meta).await?;
    Ok(info_for(&state, &dir).await?)
}

#[tauri::command]
pub async fn local_servers_start<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<()> {
    let state = app.state::<LocalServersState>();
    let dir = server_dir(&id).await?;
    let meta = read_meta(&dir).await?;

    {
        let mut running = state.running.lock().await;
        if running.contains_key(&id) {
            return Err(anyhow!("Der Server läuft bereits").into());
        }
        running.insert(
            id.clone(),
            RunningServer {
                status: ServerStatus::Starting,
                stdin: None,
                kill: None,
            },
        );
    }
    let _ = app.emit(
        "local-server://status",
        StatusEvent {
            id: &id,
            status: ServerStatus::Starting,
        },
    );

    let spawn_result = async {
        state.logs.lock().await.insert(id.clone(), VecDeque::new());
        push_log(
            &app,
            &state,
            &id,
            &format!(
                "[Modrinth Next] Java {} wird vorbereitet…",
                meta.java_major
            ),
        )
        .await;
        let java = resolve_java(meta.java_major).await?;

        let mut cmd = java_command(&java);
        cmd.current_dir(&dir)
            .arg(format!("-Xmx{}M", meta.memory_mb))
            .arg(format!("-Xms{}M", (meta.memory_mb / 2).max(512)))
            .args(&meta.jvm_args);
        match &meta.launch {
            LaunchTarget::Jar(jar) => {
                cmd.arg("-jar").arg(jar);
            }
            LaunchTarget::ArgFile(file) => {
                cmd.arg(format!("@{file}"));
            }
        }
        cmd.arg("nogui")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        push_log(
            &app,
            &state,
            &id,
            &format!(
                "[Modrinth Next] Starte {} {} mit {} MB RAM",
                meta.name, meta.game_version, meta.memory_mb
            ),
        )
        .await;
        let child =
            cmd.spawn().context("Java konnte nicht gestartet werden")?;
        anyhow::Ok(child)
    }
    .await;

    let mut child = match spawn_result {
        Ok(child) => child,
        Err(e) => {
            push_log(
                &app,
                &state,
                &id,
                &format!("[Modrinth Next] Fehler: {e:#}"),
            )
            .await;
            set_status(&app, &state, &id, ServerStatus::Stopped).await;
            return Err(e.into());
        }
    };

    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (kill_tx, kill_rx) = oneshot::channel::<()>();

    {
        let mut running = state.running.lock().await;
        if let Some(s) = running.get_mut(&id) {
            s.stdin = stdin;
            s.kill = Some(kill_tx);
        }
    }

    if let Some(stdout) = stdout {
        let app = app.clone();
        let id = id.clone();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<LocalServersState>();
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.contains("]: Done (")
                    && status_of(&state, &id).await == ServerStatus::Starting
                {
                    set_status(&app, &state, &id, ServerStatus::Running).await;
                }
                push_log(&app, &state, &id, &line).await;
            }
        });
    }
    if let Some(stderr) = stderr {
        let app = app.clone();
        let id = id.clone();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<LocalServersState>();
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                push_log(&app, &state, &id, &line).await;
            }
        });
    }

    {
        let app = app.clone();
        let id = id.clone();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<LocalServersState>();
            let exit = tokio::select! {
                status = child.wait() => status.ok(),
                _ = kill_rx => {
                    let _ = child.kill().await;
                    child.wait().await.ok()
                }
            };
            let code = exit
                .and_then(|s| s.code())
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".into());
            push_log(
                &app,
                &state,
                &id,
                &format!("[Modrinth Next] Server beendet (Code {code})"),
            )
            .await;
            set_status(&app, &state, &id, ServerStatus::Stopped).await;
        });
    }

    Ok(())
}

async fn write_stdin(
    state: &LocalServersState,
    id: &str,
    command: &str,
) -> anyhow::Result<()> {
    let mut running = state.running.lock().await;
    let server = running
        .get_mut(id)
        .ok_or_else(|| anyhow!("Der Server läuft nicht"))?;
    let stdin = server
        .stdin
        .as_mut()
        .ok_or_else(|| anyhow!("Der Server startet noch"))?;
    stdin.write_all(command.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await?;
    Ok(())
}

#[tauri::command]
pub async fn local_servers_stop<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<()> {
    let state = app.state::<LocalServersState>();
    write_stdin(&state, &id, "stop").await?;
    set_status(&app, &state, &id, ServerStatus::Stopping).await;
    Ok(())
}

#[tauri::command]
pub async fn local_servers_kill<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<()> {
    let state = app.state::<LocalServersState>();
    let kill = {
        let mut running = state.running.lock().await;
        running.get_mut(&id).and_then(|s| s.kill.take())
    };
    match kill {
        Some(tx) => {
            let _ = tx.send(());
        }
        None => set_status(&app, &state, &id, ServerStatus::Stopped).await,
    }
    Ok(())
}

#[tauri::command]
pub async fn local_servers_send_command<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    command: String,
) -> Result<()> {
    let state = app.state::<LocalServersState>();
    let command = command.trim().trim_start_matches('/');
    if command.is_empty() || command.contains(['\n', '\r']) {
        return Ok(());
    }
    push_log(&app, &state, &id, &format!("> {command}")).await;
    write_stdin(&state, &id, command).await?;
    Ok(())
}

#[tauri::command]
pub async fn local_servers_get_logs<R: Runtime>(
    app: AppHandle<R>,
    id: String,
) -> Result<Vec<String>> {
    let state = app.state::<LocalServersState>();
    Ok(state
        .logs
        .lock()
        .await
        .get(&id)
        .map(|l| l.iter().cloned().collect())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn local_servers_get_properties(
    id: String,
) -> Result<BTreeMap<String, String>> {
    let dir = server_dir(&id).await?;
    Ok(read_properties(&dir).await)
}

#[tauri::command]
pub async fn local_servers_set_properties(
    id: String,
    properties: BTreeMap<String, String>,
) -> Result<()> {
    let dir = server_dir(&id).await?;
    let mut current = read_properties(&dir).await;
    current.extend(properties);
    write_properties(&dir, &current).await?;
    Ok(())
}

#[tauri::command]
pub async fn local_servers_get_game_versions(
    loader: ServerLoader,
    include_snapshots: bool,
) -> Result<Vec<String>> {
    let client = http()?;
    let versions = match loader {
        ServerLoader::Vanilla | ServerLoader::Fabric => {
            let manifest: MojangManifest =
                get_json(&client, MOJANG_MANIFEST).await?;
            let mut list: Vec<String> = manifest
                .versions
                .into_iter()
                .filter(|v| {
                    v.kind == "release"
                        || (include_snapshots && v.kind == "snapshot")
                })
                .map(|v| v.id)
                .collect();
            if loader == ServerLoader::Fabric {
                #[derive(Deserialize)]
                struct FabricGame {
                    version: String,
                }
                let supported: HashSet<String> = get_json::<Vec<FabricGame>>(
                    &client,
                    &format!("{FABRIC_META}/versions/game"),
                )
                .await?
                .into_iter()
                .map(|g| g.version)
                .collect();
                list.retain(|v| supported.contains(v));
            }
            list
        }
        ServerLoader::Paper => {
            #[derive(Deserialize)]
            struct PaperProject {
                versions: BTreeMap<String, Vec<String>>,
            }
            let project: PaperProject = get_json(&client, PAPER_FILL).await?;
            // Order using the Mojang manifest (newest first).
            let manifest: MojangManifest =
                get_json(&client, MOJANG_MANIFEST).await?;
            let available: HashSet<String> =
                project.versions.into_values().flatten().collect();
            manifest
                .versions
                .into_iter()
                .filter(|v| available.contains(&v.id))
                .filter(|v| include_snapshots || v.kind == "release")
                .map(|v| v.id)
                .collect()
        }
        ServerLoader::Forge | ServerLoader::Neoforge => {
            let is_forge = loader == ServerLoader::Forge;
            let url = if is_forge {
                format!("{FORGE_MAVEN}/maven-metadata.xml")
            } else {
                format!("{NEOFORGE_MAVEN}/maven-metadata.xml")
            };
            let xml = get_text(&client, &url).await?;
            let available: HashSet<String> = maven_versions(&xml)
                .iter()
                .filter_map(|v| {
                    if is_forge {
                        forge_game_version(v)
                    } else {
                        neoforge_game_version(v)
                    }
                })
                .collect();
            let manifest: MojangManifest =
                get_json(&client, MOJANG_MANIFEST).await?;
            manifest
                .versions
                .into_iter()
                .filter(|v| available.contains(&v.id))
                .filter(|v| include_snapshots || v.kind == "release")
                .map(|v| v.id)
                .collect()
        }
    };
    Ok(versions)
}

#[tauri::command]
pub async fn local_servers_get_loader_versions(
    loader: ServerLoader,
    game_version: String,
) -> Result<Vec<String>> {
    let client = http()?;
    Ok(loader_versions(&client, loader, &game_version).await?)
}

#[tauri::command]
pub async fn local_servers_get_network_info(id: String) -> Result<NetworkInfo> {
    let dir = server_dir(&id).await?;
    let port = port_of(&read_properties(&dir).await);
    // Determine the LAN address by "connecting" a UDP socket. No packets
    // are actually sent.
    let lan_address = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .ok()
        .map(|a| a.ip())
        .filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
        .map(|ip| format!("{ip}:{port}"));
    Ok(NetworkInfo {
        port,
        local_address: if port == 25565 {
            "localhost".to_string()
        } else {
            format!("localhost:{port}")
        },
        lan_address,
    })
}

// ---------------------------------------------------------------------------
// Commands: content (mods / plugins / datapacks)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn local_servers_list_content(
    id: String,
) -> Result<Vec<ContentFile>> {
    let dir = server_dir(&id).await?;
    let meta = read_meta(&dir).await?;
    let content_dir = dir.join(meta.loader.content_dir());
    let mut out = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(&content_dir).await else {
        return Ok(out);
    };
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        if !file_type.is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_string();
        let (base, enabled) = match file_name.strip_suffix(".disabled") {
            Some(base) => (base.to_string(), false),
            None => (file_name.clone(), true),
        };
        if !(base.ends_with(".jar") || base.ends_with(".zip")) {
            continue;
        }
        let size = entry.metadata().await.map(|m| m.len()).unwrap_or(0);
        let project =
            meta.projects.iter().find(|p| p.file_name == base).cloned();
        out.push(ContentFile {
            file_name,
            enabled,
            size,
            project,
        });
    }
    out.sort_by(|a, b| {
        let an = a
            .project
            .as_ref()
            .map(|p| p.title.as_str())
            .unwrap_or(&a.file_name);
        let bn = b
            .project
            .as_ref()
            .map(|p| p.title.as_str())
            .unwrap_or(&b.file_name);
        an.to_lowercase().cmp(&bn.to_lowercase())
    });
    Ok(out)
}

#[tauri::command]
pub async fn local_servers_toggle_content(
    id: String,
    file_name: String,
) -> Result<String> {
    validate_file_name(&file_name)?;
    let dir = server_dir(&id).await?;
    let meta = read_meta(&dir).await?;
    let content_dir = dir.join(meta.loader.content_dir());
    let new_name = match file_name.strip_suffix(".disabled") {
        Some(base) => base.to_string(),
        None => format!("{file_name}.disabled"),
    };
    tokio::fs::rename(
        content_dir.join(&file_name),
        content_dir.join(&new_name),
    )
    .await?;
    Ok(new_name)
}

#[tauri::command]
pub async fn local_servers_remove_content(
    id: String,
    file_name: String,
) -> Result<()> {
    validate_file_name(&file_name)?;
    let dir = server_dir(&id).await?;
    let mut meta = read_meta(&dir).await?;
    let content_dir = dir.join(meta.loader.content_dir());
    tokio::fs::remove_file(content_dir.join(&file_name)).await?;
    let base = file_name.trim_end_matches(".disabled");
    meta.projects.retain(|p| p.file_name != base);
    write_meta(&dir, &meta).await?;
    Ok(())
}

#[tauri::command]
pub async fn local_servers_search_content(
    id: String,
    query: String,
    offset: u32,
) -> Result<serde_json::Value> {
    let dir = server_dir(&id).await?;
    let meta = read_meta(&dir).await?;
    let loaders = meta.loader.modrinth_loaders();

    let mut facets: Vec<Vec<String>> = vec![
        vec![format!("project_type:{}", meta.loader.project_type())],
        vec![format!("versions:{}", meta.game_version)],
    ];
    if meta.loader != ServerLoader::Vanilla {
        facets
            .push(loaders.iter().map(|l| format!("categories:{l}")).collect());
    }
    if meta.loader != ServerLoader::Paper
        && meta.loader != ServerLoader::Vanilla
    {
        facets.push(vec![
            "server_side:required".into(),
            "server_side:optional".into(),
        ]);
    }

    let client = http()?;
    let facets = serde_json::to_string(&facets)?;
    let offset = offset.to_string();
    let res = client
        .get(format!("{MODRINTH_API}/search"))
        .query(&[
            ("query", query.as_str()),
            ("facets", facets.as_str()),
            ("limit", "20"),
            ("offset", offset.as_str()),
            ("index", "relevance"),
        ])
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(anyhow!("Suche fehlgeschlagen ({})", res.status()).into());
    }
    Ok(res.json().await?)
}

#[derive(Deserialize, Debug, Clone)]
struct MrVersion {
    id: String,
    project_id: String,
    version_number: String,
    files: Vec<MrFile>,
    #[serde(default)]
    dependencies: Vec<MrDependency>,
}

#[derive(Deserialize, Debug, Clone)]
struct MrFile {
    url: String,
    filename: String,
    primary: bool,
}

#[derive(Deserialize, Debug, Clone)]
struct MrDependency {
    version_id: Option<String>,
    project_id: Option<String>,
    dependency_type: String,
}

#[derive(Deserialize, Debug, Clone)]
struct MrProject {
    id: String,
    title: String,
    icon_url: Option<String>,
}

async fn compatible_version(
    client: &reqwest::Client,
    project_id: &str,
    meta: &LocalServerMeta,
) -> anyhow::Result<MrVersion> {
    let loaders = serde_json::to_string(meta.loader.modrinth_loaders())?;
    let game_versions = serde_json::to_string(&[&meta.game_version])?;
    let res = client
        .get(format!("{MODRINTH_API}/project/{project_id}/version"))
        .query(&[
            ("loaders", loaders.as_str()),
            ("game_versions", game_versions.as_str()),
        ])
        .send()
        .await?;
    if !res.status().is_success() {
        bail!(
            "Projekt {project_id} konnte nicht geladen werden ({})",
            res.status()
        );
    }
    let versions: Vec<MrVersion> = res.json().await?;
    versions.into_iter().next().ok_or_else(|| {
        anyhow!(
            "Keine passende Version für {} {} gefunden",
            meta.loader.project_type(),
            meta.game_version
        )
    })
}

#[tauri::command]
pub async fn local_servers_install_project(
    id: String,
    project_id: String,
) -> Result<Vec<String>> {
    let dir = server_dir(&id).await?;
    let mut meta = read_meta(&dir).await?;
    let content_dir = dir.join(meta.loader.content_dir());
    tokio::fs::create_dir_all(&content_dir).await?;
    let client = http()?;

    let installed_ids: HashSet<String> =
        meta.projects.iter().map(|p| p.project_id.clone()).collect();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(Option<String>, Option<String>)> =
        VecDeque::from([(Some(project_id.clone()), None)]);
    let mut installed = Vec::new();

    while let Some((pid, vid)) = queue.pop_front() {
        let version = match (&pid, &vid) {
            (_, Some(vid)) => {
                get_json::<MrVersion>(
                    &client,
                    &format!("{MODRINTH_API}/version/{vid}"),
                )
                .await?
            }
            (Some(pid), None) => {
                match compatible_version(&client, pid, &meta).await {
                    Ok(v) => v,
                    // Missing dependencies shouldn't block the main install.
                    Err(e) if pid != &project_id => {
                        tracing::warn!("Skipping dependency {pid}: {e:#}");
                        continue;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            (None, None) => continue,
        };

        if !visited.insert(version.project_id.clone()) {
            continue;
        }
        // Already installed (and not the project the user explicitly picked)
        if version.project_id != project_id
            && installed_ids.contains(&version.project_id)
        {
            continue;
        }

        let file = version
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| version.files.first())
            .ok_or_else(|| anyhow!("Version has no files"))?;
        validate_file_name(&file.filename)?;

        let project: MrProject = get_json(
            &client,
            &format!("{MODRINTH_API}/project/{}", version.project_id),
        )
        .await?;

        // Replace older files of the same project
        if let Some(old) = meta
            .projects
            .iter()
            .find(|p| p.project_id == project.id)
            .map(|p| p.file_name.clone())
        {
            let _ = tokio::fs::remove_file(content_dir.join(&old)).await;
            let _ = tokio::fs::remove_file(
                content_dir.join(format!("{old}.disabled")),
            )
            .await;
            meta.projects.retain(|p| p.project_id != project.id);
        }

        download_to(&client, &file.url, &content_dir.join(&file.filename))
            .await?;
        meta.projects.push(InstalledProject {
            project_id: project.id.clone(),
            version_id: version.id.clone(),
            file_name: file.filename.clone(),
            title: project.title.clone(),
            icon_url: project.icon_url.clone(),
            version_number: version.version_number.clone(),
        });
        installed.push(project.title);

        for dep in &version.dependencies {
            if dep.dependency_type == "required" {
                queue.push_back((
                    dep.project_id.clone(),
                    dep.version_id.clone(),
                ));
            }
        }
    }

    write_meta(&dir, &meta).await?;
    Ok(installed)
}
