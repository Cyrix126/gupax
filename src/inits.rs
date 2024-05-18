use crate::components::update::Update;
use crate::helper::{Helper, ProcessSignal};
use crate::utils::constants::{
    APP_MAX_HEIGHT, APP_MAX_WIDTH, APP_MIN_HEIGHT, APP_MIN_WIDTH, BYTES_ICON,
};
use crate::utils::regex::Regexes;
use std::io::Write;
//---------------------------------------------------------------------------------------------------- Init functions
use crate::app::App;
use crate::{components::node::Ping, miscs::clamp_scale};
use crate::{disk::state::*, utils::macros::lock};
use crate::{info, warn};
use eframe::NativeOptions;
use egui::TextStyle::Small;
use egui::TextStyle::{Body, Button, Heading, Monospace, Name};
use egui::*;
use env_logger::fmt::style::Style;
use env_logger::{Builder, WriteStyle};
use flexi_logger::{FileSpec, Logger};
use log::LevelFilter;
use std::sync::Arc;
use std::time::Instant;
#[cold]
#[inline(never)]
pub fn init_text_styles(ctx: &egui::Context, width: f32, pixels_per_point: f32) {
    let scale = width / 35.5;
    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (Small, FontId::new(scale / 3.0, egui::FontFamily::Monospace)),
        (Body, FontId::new(scale / 2.0, egui::FontFamily::Monospace)),
        (
            Button,
            FontId::new(scale / 2.0, egui::FontFamily::Monospace),
        ),
        (
            Monospace,
            FontId::new(scale / 2.0, egui::FontFamily::Monospace),
        ),
        (
            Heading,
            FontId::new(scale / 1.5, egui::FontFamily::Monospace),
        ),
        (
            Name("Tab".into()),
            FontId::new(scale * 1.2, egui::FontFamily::Monospace),
        ),
        (
            Name("Bottom".into()),
            FontId::new(scale / 2.0, egui::FontFamily::Monospace),
        ),
        (
            Name("MonospaceSmall".into()),
            FontId::new(scale / 2.5, egui::FontFamily::Monospace),
        ),
        (
            Name("MonospaceLarge".into()),
            FontId::new(scale / 1.5, egui::FontFamily::Monospace),
        ),
    ]
    .into();
    style.spacing.icon_width_inner = width / 35.0;
    style.spacing.icon_width = width / 25.0;
    style.spacing.icon_spacing = 20.0;
    style.spacing.scroll = egui::style::ScrollStyle {
        bar_width: width / 150.0,
        ..egui::style::ScrollStyle::solid()
    };
    ctx.set_style(style);
    // Make sure scale f32 is a regular number.
    let pixels_per_point = clamp_scale(pixels_per_point);
    ctx.set_pixels_per_point(pixels_per_point);
    ctx.request_repaint();
}

#[cold]
#[inline(never)]
pub fn init_logger(now: Instant) {
    let filter_env = std::env::var("RUST_LOG").unwrap_or_else(|_| "INFO".to_string());
    let filter = match filter_env.as_str() {
        "error" | "Error" | "ERROR" => LevelFilter::Error,
        "warn" | "Warn" | "WARN" => LevelFilter::Warn,
        "debug" | "Debug" | "DEBUG" => LevelFilter::Debug,
        "trace" | "Trace" | "TRACE" => LevelFilter::Trace,
        _ => LevelFilter::Info,
    };
    std::env::set_var("RUST_LOG", format!("off,gupax={}", filter_env));
    Logger::try_with_str("info")
        .unwrap()
        .log_to_file(FileSpec::default())
        .start()
        .unwrap();
    // Builder::new()
    //     .format(move |buf, record| {
    //         let level = record.level();
    //         let level_style = buf.default_level_style(level);
    //         let dimmed = Style::new().dimmed();
    //         writeln!(
    //             buf,
    //             "{level_style}[{}]{level_style:#} [{dimmed}{:.3}{dimmed:#}] [{dimmed}{}{dimmed:#}:{dimmed}{}{dimmed:#}] {}",
    //             level,
    //             now.elapsed().as_secs_f32(),
    //             record.file().unwrap_or("???"),
    //             record.line().unwrap_or(0),
    //             record.args(),
    //         )
    //     })
    //     .filter_level(filter)
    //     .write_style(WriteStyle::Always)
    //     .parse_default_env()
    //     .format_timestamp_millis()
    //     .init();
    info!("init_logger() ... OK");
    info!("Log level ... {}", filter);
}

#[cold]
#[inline(never)]
pub fn init_options(initial_window_size: Option<Vec2>) -> NativeOptions {
    let mut options = eframe::NativeOptions::default();
    options.viewport.min_inner_size = Some(Vec2::new(APP_MIN_WIDTH, APP_MIN_HEIGHT));
    options.viewport.max_inner_size = Some(Vec2::new(APP_MAX_WIDTH, APP_MAX_HEIGHT));
    options.viewport.inner_size = initial_window_size;
    options.follow_system_theme = false;
    options.default_theme = eframe::Theme::Dark;
    let icon = image::load_from_memory(BYTES_ICON)
        .expect("Failed to read icon bytes")
        .to_rgba8();
    let (icon_width, icon_height) = icon.dimensions();
    options.viewport.icon = Some(Arc::new(egui::viewport::IconData {
        rgba: icon.into_raw(),
        width: icon_width,
        height: icon_height,
    }));
    info!("init_options() ... OK");
    options
}

#[cold]
#[inline(never)]
pub fn init_auto(app: &mut App) {
    // Return early if [--no-startup] was not passed
    if app.no_startup {
        info!("[--no-startup] flag passed, skipping init_auto()...");
        return;
    } else if app.error_state.error {
        info!("App error detected, skipping init_auto()...");
        return;
    } else {
        info!("Starting init_auto()...");
    }
    // update the absolute path, or gupaxx will crash if it's not valid and p2pool is enabled since it only verify the relative path.
    // it could be the case if gupaxx was manually installed, the relative path stay the same but absolute path will also still stay on the old path that maybe is deleted. SO the check with the absolute path would be valid but when launched with the other old/wrong path from absolute, it would panic.
    // this change is non breaking and will fix the issue if it was occurring.
    app.state
        .update_absolute_path()
        .expect("could not get the current path");

    // [Auto-Update]
    #[cfg(not(feature = "distro"))]
    if app.state.gupax.auto_update {
        Update::spawn_thread(
            &app.og,
            &app.state.gupax,
            &app.state_path,
            &app.update,
            &mut app.error_state,
            &app.restart,
        );
    } else {
        info!("Skipping auto-update...");
    }

    // [Auto-Ping]
    if app.state.p2pool.auto_ping && app.state.p2pool.simple {
        Ping::spawn_thread(&app.ping)
    } else {
        info!("Skipping auto-ping...");
    }

    // [Auto-P2Pool]
    if app.state.gupax.auto_p2pool {
        if !Regexes::addr_ok(&app.state.p2pool.address) {
            warn!("Gupaxx | P2Pool address is not valid! Skipping auto-p2pool...");
        } else if !Gupax::path_is_file(&app.state.gupax.p2pool_path) {
            warn!("Gupaxx | P2Pool path is not a file! Skipping auto-p2pool...");
        } else if !crate::components::update::check_p2pool_path(&app.state.gupax.p2pool_path) {
            warn!("Gupaxx | P2Pool path is not valid! Skipping auto-p2pool...");
        } else {
            let backup_hosts = app.gather_backup_hosts();
            Helper::start_p2pool(
                &app.helper,
                &app.state.p2pool,
                &app.state.gupax.absolute_p2pool_path,
                backup_hosts,
            );
        }
    } else {
        info!("Skipping auto-p2pool...");
    }

    // [Auto-XMRig]
    if app.state.gupax.auto_xmrig {
        if !Gupax::path_is_file(&app.state.gupax.xmrig_path) {
            warn!("Gupaxx | XMRig path is not an executable! Skipping auto-xmrig...");
        } else if !crate::components::update::check_xmrig_path(&app.state.gupax.xmrig_path) {
            warn!("Gupaxx | XMRig path is not valid! Skipping auto-xmrig...");
        } else if cfg!(windows) {
            Helper::start_xmrig(
                &app.helper,
                &app.state.xmrig,
                &app.state.gupax.absolute_xmrig_path,
                Arc::clone(&app.sudo),
            );
        } else {
            lock!(app.sudo).signal = ProcessSignal::Start;
            app.error_state.ask_sudo(&app.sudo);
        }
    } else {
        info!("Skipping auto-xmrig...");
    }
    // [Auto-XvB]
    if app.state.gupax.auto_xvb {
        Helper::start_xvb(
            &app.helper,
            &app.state.xvb,
            &app.state.p2pool,
            &app.state.xmrig,
        );
    } else {
        info!("Skipping auto-xvb...");
    }
}
