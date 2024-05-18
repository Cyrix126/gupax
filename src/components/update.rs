// Gupax - GUI Uniting P2Pool And XMRig
//
// Copyright (c) 2022-2023 hinto-janai
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

// This file contains all (most) of the code for updating.
// The main [Update] struct contains meta update information.
// It is held by the top [App] struct. Each package also gets
// a [Pkg] struct that only lasts as long as the download.
//
// An update is triggered by either:
//     a. user clicks update on [Gupax] tab
//     b. auto-update at startup

//---------------------------------------------------------------------------------------------------- Imports
use crate::{
    app::Restart,
    constants::GUPAX_VERSION,
    disk::{state::State, *},
    macros::*,
    miscs::get_exe_dir,
    utils::errors::{ErrorButtons, ErrorFerris, ErrorState},
};
use anyhow::{anyhow, Error};
use log::*;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use reqwest::header::{LOCATION, USER_AGENT};
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;

#[cfg(target_os = "windows")]
use zip::ZipArchive;
//#[cfg(target_family = "unix")]
//use std::os::unix::fs::OpenOptionsExt;

//---------------------------------------------------------------------------------------------------- Constants
// Package naming schemes:
// gupax  | gupax-vX.X.X-(windows|macos|linux)-(x64|arm64)-(standalone|bundle).(zip|tar.gz)
// Download link = PREFIX + Version (found at runtime) + SUFFIX + Version + EXT
// Example: https://github.com/hinto-janai/gupax/releases/download/v0.0.1/gupax-v0.0.1-linux-standalone-x64.tar.gz
//

const GUPAX_METADATA: &str = "https://api.github.com/repos/Cyrix126/gupaxx/releases/latest";

cfg_if::cfg_if! {
     if #[cfg(target_family = "unix")] {
    pub(super) const GUPAX_BINARY: &str = "gupaxx";
    pub(super) const P2POOL_BINARY: &str = "p2pool";
    pub(super) const XMRIG_BINARY: &str = "xmrig";
     }
}
cfg_if::cfg_if! {
     if #[cfg(target_os = "windows")] {
    pub(super) const OS_TARGET: &str = "windows";
    pub(super) const ARCHIVE_EXT: &str = "zip";
    pub(super) const GUPAX_BINARY: &str = "Gupaxx.exe";
    pub(super) const P2POOL_BINARY: &str = "p2pool.exe";
    pub(super) const XMRIG_BINARY: &str = "xmrig.exe";
     } else if #[cfg(target_os = "linux")] {
    pub(super) const OS_TARGET: &str = "linux";
    pub(super) const ARCHIVE_EXT: &str = "tar.gz";
     } else if #[cfg(target_os = "macos")] {
    pub(super) const OS_TARGET: &str = "macos";
    pub(super) const ARCHIVE_EXT: &str = "tar.gz";
     }
}

#[cfg(target_arch = "x86_64")]
pub(super) const ARCH_TARGET: &str = "x64";
#[cfg(target_arch = "aarch64")]
pub(super) const ARCH_TARGET: &str = "arm64";

// Some fake Curl/Wget user-agents because GitHub API requires one// user-agent might be fingerprintable without all the associated headers.
const FAKE_USER_AGENT: [&str; 25] = [
    "Wget/1.16.3",
    "Wget/1.17",
    "Wget/1.17.1",
    "Wget/1.18",
    "Wget/1.18",
    "Wget/1.19",
    "Wget/1.19.1",
    "Wget/1.19.2",
    "Wget/1.19.3",
    "Wget/1.19.4",
    "Wget/1.19.5",
    "Wget/1.20",
    "Wget/1.20.1",
    "Wget/1.20.2",
    "Wget/1.20.3",
    "Wget/1.21",
    "Wget/1.21.1",
    "Wget/1.21.2",
    "Wget/1.21.3",
    "Wget/1.21.4",
    "curl/7.65.3",
    "curl/7.66.0",
    "curl/7.67.0",
    "curl/7.68.0",
    "curl/8.4.0",
];

const MSG_NONE: &str = "No update in progress";
const MSG_START: &str = "Starting update";
const MSG_TMP: &str = "Creating temporary directory";
const MSG_HTTPS: &str = "Creating HTTPS client";
const MSG_METADATA: &str = "Fetching package metadata";
const MSG_COMPARE: &str = "Compare package versions";
const MSG_UP_TO_DATE: &str = "All packages already up-to-date";
const MSG_DOWNLOAD: &str = "Downloading packages";
const MSG_EXTRACT: &str = "Extracting packages";
const MSG_UPGRADE: &str = "Upgrading packages";
pub const MSG_FAILED: &str = "Update failed";
pub const MSG_FAILED_HELP: &str =
    "Consider manually replacing your executable from github releases: https://github.com/Cyrix126/gupaxx/releases";
const INIT: &str = "------------------- Init -------------------";
const METADATA: &str = "----------------- Metadata -----------------";
const COMPARE: &str = "----------------- Compare ------------------";
const DOWNLOAD: &str = "----------------- Download -----------------";
const EXTRACT: &str = "----------------- Extract ------------------";
const UPGRADE: &str = "----------------- Upgrade ------------------";

//---------------------------------------------------------------------------------------------------- General functions
pub fn check_p2pool_path(path: &str) -> bool {
    let path = match crate::disk::into_absolute_path(path.to_string()) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let path = match path.file_name() {
        Some(p) => p,
        None => {
            error!("Couldn't get P2Pool file name");
            return false;
        }
    };

    path == P2POOL_BINARY
}

pub fn check_xmrig_path(path: &str) -> bool {
    let path = match crate::disk::into_absolute_path(path.to_string()) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let path = match path.file_name() {
        Some(p) => p,
        None => {
            error!("Couldn't get XMRig file name");
            return false;
        }
    };
    path == XMRIG_BINARY
}

//---------------------------------------------------------------------------------------------------- Update struct/impl
// Contains values needed during update
// Progress bar structure:
// 0%  | Start
// 5%  | Create tmp directory, pkg list, fake user-agent
// 5%  | Create HTTPS client
// 30% | Download Metadata (x3)
// 5%  | Compare Versions (x3)
// 30% | Download Archive (x3)
// 5%  | Extract (x3)
// 5%  | Upgrade (x3)

#[derive(Clone)]
pub struct Update {
    pub path_gupax: String,  // Full path to current gupax
    pub path_p2pool: String, // Full path to current p2pool
    pub path_xmrig: String,  // Full path to current xmrig
    #[cfg(target_os = "windows")]
    pub tmp_dir: String, // Full path to temporary directory
    pub updating: Arc<Mutex<bool>>, // Is an update in progress?
    pub prog: Arc<Mutex<f32>>, // Holds the 0-100% progress bar number
    pub msg: Arc<Mutex<String>>, // Message to display on [Gupax] tab while updating
}

impl Update {
    // Takes in current paths from [State]
    pub fn new(path_gupax: String, path_p2pool: PathBuf, path_xmrig: PathBuf) -> Self {
        Self {
            path_gupax,
            path_p2pool: path_p2pool.display().to_string(),
            path_xmrig: path_xmrig.display().to_string(),
            #[cfg(target_os = "windows")]
            tmp_dir: "".to_string(),
            updating: arc_mut!(false),
            prog: arc_mut!(0.0),
            msg: arc_mut!(MSG_NONE.to_string()),
        }
    }

    // Get a temporary random folder for package download contents
    // This used to use [std::env::temp_dir()] but there were issues
    // using [std::fs::rename()] on tmpfs -> disk (Invalid cross-device link (os error 18)).
    // So, uses the [Gupax] binary directory as a base, something like [/home/hinto/gupax/gupax_update_SG4xsDdVmr]
    pub fn get_tmp_dir() -> Result<String, anyhow::Error> {
        let rand_string: String = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(10)
            .map(char::from)
            .collect();
        let base = get_exe_dir()?;
        #[cfg(target_os = "windows")]
        let tmp_dir = format!("{}{}{}{}", base, r"\gupaxx_update_", rand_string, r"\");
        #[cfg(target_family = "unix")]
        let tmp_dir = format!("{}{}{}{}", base, "/gupaxx_update_", rand_string, "/");
        info!("Update | Temporary directory ... {}", tmp_dir);
        Ok(tmp_dir)
    }

    #[cold]
    #[inline(never)]
    // Intermediate function that spawns a new thread
    // which starts the async [start()] function that
    // actually contains the code. This is so that everytime
    // an update needs to happen (Gupax tab, auto-update), the
    // code only needs to be edited once, here.
    pub fn spawn_thread(
        og: &Arc<Mutex<State>>,
        gupax: &crate::disk::state::Gupax,
        state_path: &Path,
        update: &Arc<Mutex<Update>>,
        error_state: &mut ErrorState,
        restart: &Arc<Mutex<Restart>>,
    ) {
        // We really shouldn't be in the function for
        // the Linux distro Gupax (UI gets disabled)
        // but if somehow get in here, just return.
        #[cfg(feature = "distro")]
        error!("Update | This is the [Linux distro] version of Gupax, updates are disabled");
        #[cfg(feature = "distro")]
        return;
        // verify validity of absolute path for p2pool and xmrig only if we want to update them.
        if lock!(og).gupax.bundled {
            // Check P2Pool path for safety
            // Attempt relative to absolute path
            // it's ok if file doesn't exist. User could enable bundled version for the first time.
            let p2pool_path = match into_absolute_path(gupax.p2pool_path.clone()) {
                Ok(p) => p,
                Err(e) => {
                    error_state.set(
                        format!(
                            "Provided P2Pool path could not be turned into an absolute path: {}",
                            e
                        ),
                        ErrorFerris::Error,
                        ErrorButtons::Okay,
                    );
                    return;
                }
            };
            // Check XMRig path for safety
            let xmrig_path = match into_absolute_path(gupax.xmrig_path.clone()) {
                Ok(p) => p,
                Err(e) => {
                    error_state.set(
                        format!(
                            "Provided XMRig path could not be turned into an absolute path: {}",
                            e
                        ),
                        ErrorFerris::Error,
                        ErrorButtons::Okay,
                    );
                    return;
                }
            };
            lock!(update).path_p2pool = p2pool_path.display().to_string();
            lock!(update).path_xmrig = xmrig_path.display().to_string();
        }

        // Clone before thread spawn
        let og = Arc::clone(og);
        let state_ver = Arc::clone(&lock!(og).version);
        let state_path = state_path.to_path_buf();
        let update = Arc::clone(update);
        let restart = Arc::clone(restart);
        info!("Spawning update thread...");
        std::thread::spawn(move || {
            match Update::start(update.clone(), og.clone(), restart) {
                Ok(_) => {
                    info!("Update | Saving state...");
                    let original_version = lock!(og).version.clone();
                    lock!(og).version = state_ver;
                    match State::save(&mut lock!(og), &state_path) {
                        Ok(_) => info!("Update ... OK"),
                        Err(e) => {
                            warn!("Update | Saving state ... FAIL: {}", e);
                            lock!(og).version = original_version;
                            *lock2!(update, msg) =
                                "Saving new versions into state failed".to_string();
                        }
                    };
                }
                Err(e) => {
                    info!("Update ... FAIL: {}", e);
                    *lock2!(update, msg) = format!("{} | {}\n{}", MSG_FAILED, e, MSG_FAILED_HELP);
                }
            };
            *lock2!(update, updating) = false;
        });
    }

    #[cold]
    #[inline(never)]
    // Download process:
    // 0. setup tor, client, http, etc
    // 1. fill vector with all enums
    // 2. loop over vec, download metadata
    // 3. if current == version, remove from vec
    // 4. loop over vec, download links
    // 5. extract, upgrade
    #[allow(clippy::await_holding_lock)]
    #[tokio::main]
    pub async fn start(
        update: Arc<Mutex<Self>>,
        og: Arc<Mutex<State>>,
        restart: Arc<Mutex<Restart>>,
    ) -> Result<(), anyhow::Error> {
        #[cfg(feature = "distro")]
        error!("Update | This is the [Linux distro] version of Gupaxx, updates are disabled");
        #[cfg(feature = "distro")]
        return Err(anyhow!(
            "This is the [Linux distro] version of Gupaxx, updates are disabled"
        ));

        //---------------------------------------------------------------------------------------------------- Init
        *lock2!(update, updating) = true;
        // Set timer
        let now = std::time::Instant::now();

        // Set progress bar
        *lock2!(update, msg) = MSG_START.to_string();
        *lock2!(update, prog) = 0.0;
        info!("Update | {}", INIT);

        // Get temporary directory
        let msg = MSG_TMP.to_string();
        info!("Update | {}", msg);
        *lock2!(update, msg) = msg;
        let tmp_dir = Self::get_tmp_dir()?;
        std::fs::create_dir(&tmp_dir)?;

        // Generate fake user-agent
        let user_agent = get_user_agent();
        *lock2!(update, prog) = 5.0;

        // Create HTTPS client
        let lock = lock!(update);
        let msg = MSG_HTTPS.to_string();
        info!("Update | {}", msg);
        *lock!(lock.msg) = msg;
        drop(lock);
        let client = Client::new();
        *lock2!(update, prog) += 5.0;
        info!("Update | Init ... OK ... {}%", lock2!(update, prog));

        //---------------------------------------------------------------------------------------------------- Metadata
        *lock2!(update, msg) = MSG_METADATA.to_string();
        info!("Update | {}", METADATA);
        // Loop process:
        // reqwest will retry himself
        // Send to async
        let new_ver = if let Ok(new_ver) =
            get_metadata(&client, GUPAX_METADATA.to_string(), user_agent).await
        {
            new_ver
        } else {
            error!("Update | Metadata ... FAIL");
            return Err(anyhow!("Metadata fetch failed"));
        };

        *lock2!(update, prog) += 10.0;
        info!("Update | Gupaxx {} ... OK", new_ver);

        //---------------------------------------------------------------------------------------------------- Compare
        *lock2!(update, msg) = MSG_COMPARE.to_string();
        info!("Update | {}", COMPARE);
        let diff = GUPAX_VERSION != new_ver;
        if diff {
            info!(
                "Update | Gupaxx {} != {} ... ADDING",
                GUPAX_VERSION, new_ver
            );
        } else {
            info!(
                "Update | Gupaxx {} == {} ... SKIPPING",
                GUPAX_VERSION, new_ver
            );
            info!("Update | All packages up-to-date ... RETURNING");
            *lock2!(update, prog) = 100.0;
            *lock2!(update, msg) = MSG_UP_TO_DATE.to_string();
            return Ok(());
        }
        *lock2!(update, prog) += 5.0;
        info!("Update | Compare ... OK ... {}%", lock2!(update, prog));

        // Return if 0 (all packages up-to-date)
        // Get amount of packages to divide up the percentage increases

        //---------------------------------------------------------------------------------------------------- Download
        *lock2!(update, msg) = format!("{} Gupaxx", MSG_DOWNLOAD);
        info!("Update | {}", DOWNLOAD);
        // Clone data before async
        let version = new_ver;
        // Download link = PREFIX + Version (found at runtime) + SUFFIX + Version + EXT
        // Example: https://github.com/Cyrix126/gupaxx/releases/download/v1.0.0/gupaxx-v1.0.0-linux-x64-standalone.tar.gz
        // prefix: https://github.com/Cyrix126/gupaxx/releases/download
        // version: v1.0.0
        // suffix: gupaxx
        // version: v1.0.0
        // os
        // arch
        // standalone or bundled
        // archive extension
        let bundle = if lock!(og).gupax.bundled {
            "bundle"
        } else {
            "standalone"
        };
        let link = [
            "https://github.com/Cyrix126/gupaxx/releases/download/",
            &version,
            "/gupaxx-",
            &version,
            "-",
            OS_TARGET,
            "-",
            ARCH_TARGET,
            "-",
            bundle,
            ".",
            ARCHIVE_EXT,
        ]
        .concat();
        info!("Update | Gupaxx ... {}", link);
        let bytes = if let Ok(bytes) = get_bytes(&client, link, user_agent).await {
            bytes
        } else {
            error!("Update | Download ... FAIL");
            return Err(anyhow!("Download failed"));
        };
        *lock2!(update, prog) += 30.0;
        info!("Update | Gupaxx ... OK");
        info!("Update | Download ... OK ... {}%", *lock2!(update, prog));

        //---------------------------------------------------------------------------------------------------- Extract
        *lock2!(update, msg) = format!("{} Gupaxx", MSG_EXTRACT);
        info!("Update | {}", EXTRACT);
        let tmp = tmp_dir.to_owned();
        #[cfg(target_os = "windows")]
        ZipArchive::extract(
            &mut ZipArchive::new(std::io::Cursor::new(bytes.as_ref()))?,
            tmp,
        )?;
        #[cfg(target_family = "unix")]
        tar::Archive::new(flate2::read::GzDecoder::new(bytes.as_ref())).unpack(tmp)?;
        *lock2!(update, prog) += 5.0;
        info!("Update | Gupaxx ... OK");
        info!("Update | Extract ... OK ... {}%", *lock2!(update, prog));

        //---------------------------------------------------------------------------------------------------- Upgrade
        // if bundled, directories p2pool and xmrig will exist.
        // if not, only gupaxx binary will be present.
        // 1. Walk directories
        //
        // 3. Rename tmp path into current path
        // 4. Update [State/Version]
        *lock2!(update, msg) = format!("Gupaxx {}", MSG_UPGRADE);
        info!("Update | {}", UPGRADE);
        // If this bool doesn't get set, something has gone wrong because
        // we _didn't_ find a binary even though we downloaded it.
        let mut found = false;
        for entry in WalkDir::new(tmp_dir.clone()) {
            let entry = entry?.clone();
            // If not a file, continue
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry
                .file_name()
                .to_str()
                .ok_or_else(|| anyhow!("WalkDir basename failed"))?;
            let path = match name {
                GUPAX_BINARY => lock!(update).path_gupax.clone(),
                P2POOL_BINARY => lock!(update).path_p2pool.clone(),
                XMRIG_BINARY => lock!(update).path_xmrig.clone(),
                _ => continue,
            };
            found = true;
            let path = Path::new(&path);
            // Unix can replace running binaries no problem (they're loaded into memory)
            // Windows locks binaries in place, so we must move (rename) current binary
            // into the temp folder, then move the new binary into the old ones spot.
            // Clearing the temp folder is now moved at startup instead at the end
            // of this function due to this behavior, thanks Windows.
            #[cfg(target_os = "windows")]
            if path.exists() {
                let tmp_windows = match name {
                    GUPAX_BINARY => tmp_dir.clone() + "gupaxx_old.exe",
                    P2POOL_BINARY => tmp_dir.clone() + "p2pool_old.exe",
                    XMRIG_BINARY => tmp_dir.clone() + "xmrig_old.exe",
                    _ => continue,
                };
                info!(
                    "Update | WINDOWS ONLY ... Moving old [{}] -> [{}]",
                    path.display(),
                    tmp_windows
                );
                std::fs::rename(&path, tmp_windows)?;
            }
            info!(
                "Update | Moving new [{}] -> [{}]",
                entry.path().display(),
                path.display()
            );
            // if bundled, create directory for p2pool and xmrig if not present
            if lock!(og).gupax.bundled && (name == P2POOL_BINARY || name == XMRIG_BINARY) {
                std::fs::create_dir_all(
                    path.parent()
                        .ok_or_else(|| anyhow!(format!("{} path failed", name)))?,
                )?;
            }
            // Move downloaded path into old path
            std::fs::rename(entry.path(), path)?;
            // If we're updating Gupax, set the [Restart] state so that the user knows to restart
            *lock!(restart) = Restart::Yes;
            *lock2!(update, prog) += 5.0;
        }
        if !found {
            return Err(anyhow!("Fatal error: Package binary could not be found"));
        }

        // Remove tmp dir (on Unix)
        #[cfg(target_family = "unix")]
        info!("Update | Removing temporary directory ... {}", tmp_dir);
        #[cfg(target_family = "unix")]
        std::fs::remove_dir_all(&tmp_dir)?;

        let seconds = now.elapsed().as_secs();
        info!("Update | Seconds elapsed ... [{}s]", seconds);
        *lock2!(update, msg) = format!(
            "Updated from {} to {}\nYou need to restart Gupaxx.",
            GUPAX_VERSION, version
        );
        *lock2!(update, prog) = 100.0;
        Ok(())
    }
}

//---------------------------------------------------------------------------------------------------- Pkg functions
#[cold]
#[inline(never)]
// Generate fake [User-Agent] HTTP header
pub fn get_user_agent() -> &'static str {
    let index = FAKE_USER_AGENT.len() - 1;

    let rand = thread_rng().gen_range(0..index);
    let user_agent = FAKE_USER_AGENT[rand];
    info!("Randomly selected User-Agent ({rand}/{index}) ... {user_agent}");
    user_agent
}

#[cold]
#[inline(never)]
// Generate GET request based off input URI + fake user agent
fn get_request(
    client: &Client,
    link: String,
    user_agent: &'static str,
) -> Result<RequestBuilder, anyhow::Error> {
    Ok(client.get(link).header(USER_AGENT, user_agent))
}

#[cold]
#[inline(never)]
// Get metadata using [Generic hyper::client<C>] & [Request]
// and change [version, prog] under an Arc<Mutex>
async fn get_metadata(
    client: &Client,
    link: String,
    user_agent: &'static str,
) -> Result<String, Error> {
    let request = get_request(client, link, user_agent)?;
    let response = request.send().await?;
    let body = response.json::<TagName>().await?;
    Ok(body.tag_name)
}

#[cold]
#[inline(never)]
async fn get_bytes(
    client: &Client,
    link: String,
    user_agent: &'static str,
) -> Result<bytes::Bytes, anyhow::Error> {
    let request = get_request(client, link, user_agent)?;
    let mut response = request.send().await?;
    // GitHub sends a 302 redirect, so we must follow
    // the [Location] header... only if Reqwest had custom
    // connectors so I didn't have to manually do this...
    if response.headers().contains_key(LOCATION) {
        response = get_request(
            client,
            response
                .headers()
                .get(LOCATION)
                .ok_or_else(|| anyhow!("HTTP Location header GET failed"))?
                .to_str()?
                .to_string(),
            user_agent,
        )?
        .send()
        .await?;
    }
    let body = response.bytes().await?;
    Ok(body)
}

// This inherits the value of [tag_name] from GitHub's JSON API
#[derive(Debug, Serialize, Deserialize)]
struct TagName {
    tag_name: String,
}
