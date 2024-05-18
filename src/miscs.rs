//---------------------------------------------------------------------------------------------------- Misc functions
#[cold]
#[inline(never)]
pub fn parse_args<S: Into<String>>(mut app: App, panic: S) -> App {
    info!("Parsing CLI arguments...");
    let mut args: Vec<String> = env::args().collect();
    if args.len() == 1 {
        info!("No args ... OK");
        return app;
    } else {
        args.remove(0);
        info!("Args ... {:?}", args);
    }
    // [help/version], exit early
    for arg in &args {
        match arg.as_str() {
            "--help" => {
                println!("{}", ARG_HELP);
                exit(0);
            }
            "--version" => {
                println!("Gupaxx {} [OS: {}, Commit: {}]\nThis Gupax was originally bundled with:\n    - P2Pool {}\n    - XMRig {}\n\n{}", GUPAX_VERSION, OS_NAME, &COMMIT[..40], P2POOL_VERSION, XMRIG_VERSION, ARG_COPYRIGHT);
                exit(0);
            }
            "--ferris" => {
                println!("{}", FERRIS_ANSI);
                exit(0);
            }
            _ => (),
        }
    }
    // Abort on panic
    let panic = panic.into();
    if !panic.is_empty() {
        info!("[Gupax error] {}", panic);
        exit(1);
    }

    // Everything else
    for arg in args {
        match arg.as_str() {
            "--state" => {
                info!("Printing state...");
                print_disk_file(&app.state_path);
            }
            "--nodes" => {
                info!("Printing node list...");
                print_disk_file(&app.node_path);
            }
            "--payouts" => {
                info!("Printing payouts...\n");
                print_gupax_p2pool_api(&app.gupax_p2pool_api);
            }
            "--reset-state" => {
                if let Ok(()) = reset_state(&app.state_path) {
                    println!("\nState reset ... OK");
                    exit(0);
                } else {
                    eprintln!("\nState reset ... FAIL");
                    exit(1)
                }
            }
            "--reset-nodes" => {
                if let Ok(()) = reset_nodes(&app.node_path) {
                    println!("\nNode reset ... OK");
                    exit(0)
                } else {
                    eprintln!("\nNode reset ... FAIL");
                    exit(1)
                }
            }
            "--reset-pools" => {
                if let Ok(()) = reset_pools(&app.pool_path) {
                    println!("\nPool reset ... OK");
                    exit(0)
                } else {
                    eprintln!("\nPool reset ... FAIL");
                    exit(1)
                }
            }
            "--reset-payouts" => {
                if let Ok(()) = reset_gupax_p2pool_api(&app.gupax_p2pool_api_path) {
                    println!("\nGupaxP2poolApi reset ... OK");
                    exit(0)
                } else {
                    eprintln!("\nGupaxP2poolApi reset ... FAIL");
                    exit(1)
                }
            }
            "--reset-all" => reset(
                &app.os_data_path,
                &app.state_path,
                &app.node_path,
                &app.pool_path,
                &app.gupax_p2pool_api_path,
            ),
            "--no-startup" => app.no_startup = true,
            _ => {
                eprintln!(
                    "\n[Gupax error] Invalid option: [{}]\nFor help, use: [--help]",
                    arg
                );
                exit(1);
            }
        }
    }
    app
}

// Get absolute [Gupax] binary path
#[cold]
#[inline(never)]
pub fn get_exe() -> Result<String, std::io::Error> {
    match std::env::current_exe() {
        Ok(path) => Ok(path.display().to_string()),
        Err(err) => {
            error!("Couldn't get absolute Gupax PATH");
            Err(err)
        }
    }
}

// Get absolute [Gupax] directory path
#[cold]
#[inline(never)]
pub fn get_exe_dir() -> Result<String, std::io::Error> {
    match std::env::current_exe() {
        Ok(mut path) => {
            path.pop();
            Ok(path.display().to_string())
        }
        Err(err) => {
            error!("Couldn't get exe basepath PATH");
            Err(err)
        }
    }
}

// Clean any [gupax_update_.*] directories
// The trailing random bits must be exactly 10 alphanumeric characters
#[cold]
#[inline(never)]
pub fn clean_dir() -> Result<(), anyhow::Error> {
    let regex = Regex::new("^gupaxx_update_[A-Za-z0-9]{10}$").unwrap();
    for entry in std::fs::read_dir(get_exe_dir()?)? {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }
        if Regex::is_match(
            &regex,
            entry
                .file_name()
                .to_str()
                .ok_or_else(|| anyhow::Error::msg("Basename failed"))?,
        ) {
            let path = entry.path();
            match std::fs::remove_dir_all(&path) {
                Ok(_) => info!("Remove [{}] ... OK", path.display()),
                Err(e) => warn!("Remove [{}] ... FAIL ... {}", path.display(), e),
            }
        }
    }
    Ok(())
}

// Print disk files to console
#[cold]
#[inline(never)]
fn print_disk_file(path: &PathBuf) {
    match std::fs::read_to_string(path) {
        Ok(string) => {
            print!("{}", string);
            exit(0);
        }
        Err(e) => {
            error!("{}", e);
            exit(1);
        }
    }
}

// Prints the GupaxP2PoolApi files.
#[cold]
#[inline(never)]
pub fn print_gupax_p2pool_api(gupax_p2pool_api: &Arc<Mutex<GupaxP2poolApi>>) {
    let api = lock!(gupax_p2pool_api);
    let log = match std::fs::read_to_string(&api.path_log) {
        Ok(string) => string,
        Err(e) => {
            error!("{}", e);
            exit(1);
        }
    };
    let payout = match std::fs::read_to_string(&api.path_payout) {
        Ok(string) => string,
        Err(e) => {
            error!("{}", e);
            exit(1);
        }
    };
    let xmr = match std::fs::read_to_string(&api.path_xmr) {
        Ok(string) => string,
        Err(e) => {
            error!("{}", e);
            exit(1);
        }
    };
    let xmr = match xmr.trim().parse::<u64>() {
        Ok(o) => crate::xmr::AtomicUnit::from_u64(o),
        Err(e) => {
            warn!("GupaxP2poolApi | [xmr] parse error: {}", e);
            exit(1);
        }
    };
    println!(
        "{}\nTotal payouts | {}\nTotal XMR     | {} ({} Atomic Units)",
        log,
        payout.trim(),
        xmr,
        xmr.to_u64()
    );
    exit(0);
}

#[inline]
pub fn cmp_f64(a: f64, b: f64) -> std::cmp::Ordering {
    match (a <= b, a >= b) {
        (false, true) => std::cmp::Ordering::Greater,
        (true, false) => std::cmp::Ordering::Less,
        (true, true) => std::cmp::Ordering::Equal,
        _ => std::cmp::Ordering::Less,
    }
}
// Free functions.

use crate::disk::gupax_p2pool_api::GupaxP2poolApi;
use crate::utils::macros::lock;
use log::error;
use log::warn;
use regex::Regex;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::{env, process::exit};

use log::info;

//---------------------------------------------------------------------------------------------------- Use
use crate::{
    app::App,
    constants::*,
    utils::{
        ferris::FERRIS_ANSI,
        resets::{reset, reset_gupax_p2pool_api, reset_nodes, reset_pools, reset_state},
    },
};

//----------------------------------------------------------------------------------------------------
#[cold]
#[inline(never)]
// Clamp the scaling resolution `f32` to a known good `f32`.
pub fn clamp_scale(scale: f32) -> f32 {
    // Make sure it is finite.
    if !scale.is_finite() {
        return APP_DEFAULT_SCALE;
    }

    // Clamp between valid range.
    scale.clamp(APP_MIN_SCALE, APP_MAX_SCALE)
}
