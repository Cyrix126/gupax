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
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

// Hide console in Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Only (windows|macos|linux) + (x64|arm64) are supported.
#[cfg(not(target_pointer_width = "64"))]
compile_error!("gupax is only compatible with 64-bit CPUs");

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux",)))]
compile_error!("gupax is only built for windows/macos/linux");

use crate::app::App;
//---------------------------------------------------------------------------------------------------- Imports
use crate::constants::*;
use crate::inits::init_auto;
use crate::inits::init_logger;
use crate::inits::init_options;
use crate::miscs::clean_dir;
use crate::utils::*;
use egui::Vec2;
use log::info;
use log::warn;
use std::time::Instant;

mod app;
mod components;
mod disk;
mod helper;
mod inits;
mod miscs;
mod utils;

// Sudo (dummy values for Windows)
#[cfg(target_family = "unix")]
extern crate sudo as sudo_check;

//---------------------------------------------------------------------------------------------------- Main [App] frame
fn main() {
    let now = Instant::now();

    // Set custom panic hook.
    crate::panic::set_panic_hook(now);

    // Init logger.
    init_logger(now);
    let mut app = App::new(now);
    init_auto(&mut app);

    // Init GUI stuff.
    let selected_width = app.state.gupax.selected_width as f32;
    let selected_height = app.state.gupax.selected_height as f32;
    let initial_window_size = if selected_width > APP_MAX_WIDTH || selected_height > APP_MAX_HEIGHT
    {
        warn!("App | Set width or height was greater than the maximum! Starting with the default resolution...");
        Some(Vec2::new(APP_DEFAULT_WIDTH, APP_DEFAULT_HEIGHT))
    } else {
        Some(Vec2::new(
            app.state.gupax.selected_width as f32,
            app.state.gupax.selected_height as f32,
        ))
    };
    let options = init_options(initial_window_size);

    // Gupax folder cleanup.
    match clean_dir() {
        Ok(_) => info!("Temporary folder cleanup ... OK"),
        Err(e) => warn!("Could not cleanup [gupax_tmp] folders: {}", e),
    }

    let resolution = Vec2::new(selected_width, selected_height);

    // Run Gupax.
    info!("/*************************************/ Init ... OK /*************************************/");
    eframe::run_native(
        &app.name_version.clone(),
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Box::new(App::cc(cc, resolution, app))
        }),
    )
    .unwrap();
}
