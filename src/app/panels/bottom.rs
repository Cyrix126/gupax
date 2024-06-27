use std::sync::Arc;

use crate::app::{keys::KeyPressed, Restart};
use crate::disk::node::Node;
use crate::disk::pool::Pool;
use crate::disk::state::{Gupax, State};
use crate::disk::status::Submenu;
use crate::helper::{Helper, ProcessSignal, ProcessState};
use crate::utils::constants::*;
use crate::utils::errors::{ErrorButtons, ErrorFerris};
use crate::utils::macros::lock;
use crate::utils::regex::Regexes;
use egui::TextStyle::Name;
use egui::*;
use log::debug;

use crate::app::Tab;
use crate::helper::ProcessState::*;
impl crate::app::App {
    #[allow(clippy::too_many_arguments)]
    pub fn bottom_panel(
        &mut self,
        ctx: &egui::Context,
        p2pool_state: ProcessState,
        xmrig_state: ProcessState,
        xvb_state: ProcessState,
        key: &KeyPressed,
        wants_input: bool,
        p2pool_is_waiting: bool,
        xmrig_is_waiting: bool,
        xvb_is_waiting: bool,
        p2pool_is_alive: bool,
        xmrig_is_alive: bool,
        xvb_is_alive: bool,
    ) {
        // Bottom: app info + state/process buttons
        debug!("App | Rendering BOTTOM bar");
        TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            let height = self.size.y / 22.0;
            // let width = self.size.x;
            ui.style_mut().override_text_style = Some(Name("Bottom".into()));
            ui.horizontal(|ui| {
                ui.group(|ui| {
                    let size = vec2(0.0, height);
                    // [Gupax Version]
                    // Is yellow if the user updated and should (but isn't required to) restart.
                    match *lock!(self.restart) {
                        Restart::Yes => ui
                            .add_sized(
                                size,
                                Label::new(RichText::new(&self.name_version).color(YELLOW)),
                            )
                            .on_hover_text(GUPAX_SHOULD_RESTART),
                        _ => ui.add_sized(size, Label::new(&self.name_version)),
                    };
                    ui.separator();
                    // [OS]
                    // Check if admin for windows.
                    // Unix SHOULDN'T be running as root, and the check is done when
                    // [App] is initialized, so no reason to check here.
                    #[cfg(target_os = "windows")]
                    if self.admin {
                        ui.add_sized(size, Label::new(self.os));
                    } else {
                        ui.add_sized(size, Label::new(RichText::new(self.os).color(RED)))
                            .on_hover_text(WINDOWS_NOT_ADMIN);
                    }
                    #[cfg(target_family = "unix")]
                    // [P2Pool/XMRig/XvB] Status
                    ui.label(self.os);
                    // ui.add_sized(size, Label::new(self.os));
                    ui.separator();
                    status_p2pool(p2pool_state, ui, size);
                    ui.separator();
                    status_xmrig(xmrig_state, ui, size);
                    ui.separator();
                    status_xvb(xvb_state, ui, size);
                });

                ui.with_layout(Layout::right_to_left(Align::RIGHT), |ui| {
                    let width = (ui.available_width() / 3.0) - (SPACE * 3.0);
                    let size = vec2(width, height);
                    // [Save/Reset]
                    self.save_reset_ui(ui, size, key, wants_input);
                    // [Simple/Advanced] + [Start/Stop/Restart]
                    match self.tab {
                        Tab::Status => {
                            self.status_submenu(ui, height);
                        }
                        Tab::Gupax => {
                            self.gupax_submenu(ui, height);
                        }
                        Tab::P2pool => {
                            self.p2pool_submenu(ui, size);
                            self.p2pool_run_actions(
                                ui,
                                height,
                                p2pool_is_waiting,
                                p2pool_is_alive,
                                wants_input,
                                key,
                            );
                        }
                        Tab::Xmrig => {
                            self.xmrig_submenu(ui, size);
                            self.xmrig_run_actions(
                                ui,
                                height,
                                xmrig_is_waiting,
                                xmrig_is_alive,
                                key,
                                wants_input,
                            );
                        }
                        Tab::Xvb => {
                            self.xvb_submenu(ui, size);
                            self.xvb_run_actions(
                                ui,
                                height,
                                xvb_is_waiting,
                                xvb_is_alive,
                                key,
                                wants_input,
                            );
                        }
                        Tab::About => {}
                    }
                });
            });
        });
    }
    fn save_reset_ui(&mut self, ui: &mut Ui, size: Vec2, key: &KeyPressed, wants_input: bool) {
        ui.group(|ui| {
            ui.set_enabled(self.diff);
            let width = size.x / 2.0;
            let size = vec2(width, size.y);
            if key.is_r() && !wants_input && self.diff
                || ui
                    .add_sized(size, Button::new("Reset"))
                    .on_hover_text("Reset changes")
                    .clicked()
            {
                let og = lock!(self.og).clone();
                self.state.status = og.status;
                self.state.gupax = og.gupax;
                self.state.p2pool = og.p2pool;
                self.state.xmrig = og.xmrig;
                self.state.xvb = og.xvb;
                self.node_vec.clone_from(&self.og_node_vec);
                self.pool_vec.clone_from(&self.og_pool_vec);
            }
            if key.is_s() && !wants_input && self.diff
                || ui
                    .add_sized(size, Button::new("Save"))
                    .on_hover_text("Save changes")
                    .clicked()
            {
                match State::save(&mut self.state, &self.state_path) {
                    Ok(_) => {
                        let mut og = lock!(self.og);
                        og.status = self.state.status.clone();
                        og.gupax = self.state.gupax.clone();
                        og.p2pool = self.state.p2pool.clone();
                        og.xmrig = self.state.xmrig.clone();
                        og.xvb = self.state.xvb.clone();
                    }
                    Err(e) => {
                        self.error_state.set(
                            format!("State file: {}", e),
                            ErrorFerris::Error,
                            ErrorButtons::Okay,
                        );
                    }
                };
                match Node::save(&self.node_vec, &self.node_path) {
                    Ok(_) => self.og_node_vec.clone_from(&self.node_vec),
                    Err(e) => self.error_state.set(
                        format!("Node list: {}", e),
                        ErrorFerris::Error,
                        ErrorButtons::Okay,
                    ),
                };
                match Pool::save(&self.pool_vec, &self.pool_path) {
                    Ok(_) => self.og_pool_vec.clone_from(&self.pool_vec),
                    Err(e) => self.error_state.set(
                        format!("Pool list: {}", e),
                        ErrorFerris::Error,
                        ErrorButtons::Okay,
                    ),
                };
            }
        });
    }
    fn status_submenu(&mut self, ui: &mut Ui, height: f32) {
        // ui.style_mut().wrap = Some(true);
        ui.group(|ui| {
            let size = vec2(0.0, height);
            if ui
                .add_sized(
                    size,
                    SelectableLabel::new(
                        self.state.status.submenu == Submenu::Benchmarks,
                        "Benchmarks",
                    ),
                )
                .on_hover_text(STATUS_SUBMENU_HASHRATE)
                .clicked()
            {
                self.state.status.submenu = Submenu::Benchmarks;
            }
            ui.separator();
            if ui
                .add_sized(
                    size,
                    SelectableLabel::new(self.state.status.submenu == Submenu::P2pool, "P2Pool"),
                )
                .on_hover_text(STATUS_SUBMENU_P2POOL)
                .clicked()
            {
                self.state.status.submenu = Submenu::P2pool;
            }
            ui.separator();
            if ui
                .add_sized(
                    size,
                    SelectableLabel::new(
                        self.state.status.submenu == Submenu::Processes,
                        "Processes",
                    ),
                )
                .on_hover_text(STATUS_SUBMENU_PROCESSES)
                .clicked()
            {
                self.state.status.submenu = Submenu::Processes;
            }
        });
    }
    fn gupax_submenu(&mut self, ui: &mut Ui, height: f32) {
        ui.group(|ui| {
            let width = (ui.available_width() / 2.0) - 10.5;
            let size = vec2(width, height);
            if ui
                .add_sized(
                    size,
                    SelectableLabel::new(!self.state.gupax.simple, "Advanced"),
                )
                .on_hover_text(GUPAX_ADVANCED)
                .clicked()
            {
                self.state.gupax.simple = false;
            }
            ui.separator();
            if ui
                .add_sized(
                    size,
                    SelectableLabel::new(self.state.gupax.simple, "Simple"),
                )
                .on_hover_text(GUPAX_SIMPLE)
                .clicked()
            {
                self.state.gupax.simple = true;
            }
        });
    }
    fn p2pool_submenu(&mut self, ui: &mut Ui, size: Vec2) {
        ui.group(|ui| {
            let width = size.x / 1.5;
            let size = vec2(width, size.y);
            if ui
                .add_sized(
                    size,
                    SelectableLabel::new(!self.state.p2pool.simple, "Advanced"),
                )
                .on_hover_text(P2POOL_ADVANCED)
                .clicked()
            {
                self.state.p2pool.simple = false;
            }
            ui.separator();
            if ui
                .add_sized(
                    size,
                    SelectableLabel::new(self.state.p2pool.simple, "Simple"),
                )
                .on_hover_text(P2POOL_SIMPLE)
                .clicked()
            {
                self.state.p2pool.simple = true;
            }
        });
    }
    fn p2pool_run_actions(
        &mut self,
        ui: &mut Ui,
        height: f32,
        p2pool_is_waiting: bool,
        p2pool_is_alive: bool,
        wants_input: bool,
        key: &KeyPressed,
    ) {
        ui.group(|ui| {
            let width = (ui.available_width() / 3.0) - 5.0;
            let size = vec2(width, height);
            if p2pool_is_waiting {
                ui.add_enabled_ui(false, |ui| {
                    ui.add_sized(size, Button::new("⟲"))
                        .on_disabled_hover_text(P2POOL_MIDDLE);
                    ui.add_sized(size, Button::new("⏹"))
                        .on_disabled_hover_text(P2POOL_MIDDLE);
                    ui.add_sized(size, Button::new("▶"))
                        .on_disabled_hover_text(P2POOL_MIDDLE);
                });
            } else if p2pool_is_alive {
                if key.is_up() && !wants_input
                    || ui
                        .add_sized(size, Button::new("⟲"))
                        .on_hover_text("Restart P2Pool")
                        .clicked()
                {
                    let _ = lock!(self.og).update_absolute_path();
                    let _ = self.state.update_absolute_path();
                    Helper::restart_p2pool(
                        &self.helper,
                        &self.state.p2pool,
                        &self.state.gupax.absolute_p2pool_path,
                        self.gather_backup_hosts(),
                    );
                }
                if key.is_down() && !wants_input
                    || ui
                        .add_sized(size, Button::new("⏹"))
                        .on_hover_text("Stop P2Pool")
                        .clicked()
                {
                    Helper::stop_p2pool(&self.helper);
                }
                ui.add_enabled_ui(false, |ui| {
                    ui.add_sized(size, Button::new("▶"))
                        .on_disabled_hover_text("Start P2Pool");
                });
            } else {
                ui.add_enabled_ui(false, |ui| {
                    ui.add_sized(size, Button::new("⟲"))
                        .on_disabled_hover_text("Restart P2Pool");
                    ui.add_sized(size, Button::new("⏹"))
                        .on_disabled_hover_text("Stop P2Pool");
                });
                // Check if address and path is okay before allowing to start.
                let mut text = String::new();
                let mut ui_enabled = true;
                if !Regexes::addr_ok(&self.state.p2pool.address) {
                    ui_enabled = false;
                    text = format!("Error: {}", P2POOL_ADDRESS);
                } else if !Gupax::path_is_file(&self.state.gupax.p2pool_path) {
                    ui_enabled = false;
                    text = format!("Error: {}", P2POOL_PATH_NOT_FILE);
                } else if !crate::components::update::check_p2pool_path(
                    &self.state.gupax.p2pool_path,
                ) {
                    ui_enabled = false;
                    text = format!("Error: {}", P2POOL_PATH_NOT_VALID);
                }
                ui.set_enabled(ui_enabled);
                let color = if ui_enabled { GREEN } else { RED };
                if (ui_enabled && key.is_up() && !wants_input)
                    || ui
                        .add_sized(size, Button::new(RichText::new("▶").color(color)))
                        .on_hover_text("Start P2Pool")
                        .on_disabled_hover_text(text)
                        .clicked()
                {
                    let _ = lock!(self.og).update_absolute_path();
                    let _ = self.state.update_absolute_path();
                    Helper::start_p2pool(
                        &self.helper,
                        &self.state.p2pool,
                        &self.state.gupax.absolute_p2pool_path,
                        self.gather_backup_hosts(),
                    );
                }
            }
        });
    }
    fn xmrig_submenu(&mut self, ui: &mut Ui, size: Vec2) {
        ui.group(|ui| {
            let width = size.x / 1.5;
            let size = vec2(width, size.y);
            if ui
                .add_sized(
                    size,
                    SelectableLabel::new(!self.state.xmrig.simple, "Advanced"),
                )
                .on_hover_text(XMRIG_ADVANCED)
                .clicked()
            {
                self.state.xmrig.simple = false;
            }
            ui.separator();
            if ui
                .add_sized(
                    size,
                    SelectableLabel::new(self.state.xmrig.simple, "Simple"),
                )
                .on_hover_text(XMRIG_SIMPLE)
                .clicked()
            {
                self.state.xmrig.simple = true;
            }
        });
    }
    fn xmrig_run_actions(
        &mut self,
        ui: &mut Ui,
        height: f32,
        xmrig_is_waiting: bool,
        xmrig_is_alive: bool,
        key: &KeyPressed,
        wants_input: bool,
    ) {
        ui.group(|ui| {
            let width = (ui.available_width() / 3.0) - 5.0;
            let size = vec2(width, height);
            if xmrig_is_waiting {
                ui.add_enabled_ui(false, |ui| {
                    ui.add_sized(size, Button::new("⟲"))
                        .on_disabled_hover_text(XMRIG_MIDDLE);
                    ui.add_sized(size, Button::new("⏹"))
                        .on_disabled_hover_text(XMRIG_MIDDLE);
                    ui.add_sized(size, Button::new("▶"))
                        .on_disabled_hover_text(XMRIG_MIDDLE);
                });
            } else if xmrig_is_alive {
                if key.is_up() && !wants_input
                    || ui
                        .add_sized(size, Button::new("⟲"))
                        .on_hover_text("Restart XMRig")
                        .clicked()
                {
                    let _ = lock!(self.og).update_absolute_path();
                    let _ = self.state.update_absolute_path();
                    if cfg!(windows) {
                        Helper::restart_xmrig(
                            &self.helper,
                            &self.state.xmrig,
                            &self.state.gupax.absolute_xmrig_path,
                            Arc::clone(&self.sudo),
                        );
                    } else {
                        lock!(self.sudo).signal = ProcessSignal::Restart;
                        self.error_state.ask_sudo(&self.sudo);
                    }
                }
                if key.is_down() && !wants_input
                    || ui
                        .add_sized(size, Button::new("⏹"))
                        .on_hover_text("Stop XMRig")
                        .clicked()
                {
                    if cfg!(target_os = "macos") {
                        lock!(self.sudo).signal = ProcessSignal::Stop;
                        self.error_state.ask_sudo(&self.sudo);
                    } else {
                        Helper::stop_xmrig(&self.helper);
                    }
                }
                ui.add_enabled_ui(false, |ui| {
                    ui.add_sized(size, Button::new("▶"))
                        .on_disabled_hover_text("Start XMRig");
                });
            } else {
                ui.add_enabled_ui(false, |ui| {
                    ui.add_sized(size, Button::new("⟲"))
                        .on_disabled_hover_text("Restart XMRig");
                    ui.add_sized(size, Button::new("⏹"))
                        .on_disabled_hover_text("Stop XMRig");
                });
                let mut text = String::new();
                let mut ui_enabled = true;
                if !Gupax::path_is_file(&self.state.gupax.xmrig_path) {
                    ui_enabled = false;
                    text = format!("Error: {}", XMRIG_PATH_NOT_FILE);
                } else if !crate::components::update::check_xmrig_path(&self.state.gupax.xmrig_path)
                {
                    ui_enabled = false;
                    text = format!("Error: {}", XMRIG_PATH_NOT_VALID);
                }
                ui.set_enabled(ui_enabled);
                let color = if ui_enabled { GREEN } else { RED };
                if (ui_enabled && key.is_up() && !wants_input)
                    || ui
                        .add_sized(size, Button::new(RichText::new("▶").color(color)))
                        .on_hover_text("Start XMRig")
                        .on_disabled_hover_text(text)
                        .clicked()
                {
                    let _ = lock!(self.og).update_absolute_path();
                    let _ = self.state.update_absolute_path();
                    if cfg!(windows) {
                        Helper::start_xmrig(
                            &self.helper,
                            &self.state.xmrig,
                            &self.state.gupax.absolute_xmrig_path,
                            Arc::clone(&self.sudo),
                        );
                    } else if cfg!(unix) {
                        lock!(self.sudo).signal = ProcessSignal::Start;
                        self.error_state.ask_sudo(&self.sudo);
                    }
                }
            }
        });
    }

    fn xvb_submenu(&mut self, ui: &mut Ui, size: Vec2) {
        ui.group(|ui| {
            let width = size.x / 1.5;
            let size = vec2(width, size.y);
            if ui
                .add_sized(
                    size,
                    SelectableLabel::new(!self.state.xvb.simple, "Advanced"),
                )
                .clicked()
            {
                self.state.xvb.simple = false;
            }
            ui.separator();
            if ui
                .add_sized(size, SelectableLabel::new(self.state.xvb.simple, "Simple"))
                .clicked()
            {
                self.state.xvb.simple = true;
            }
        });
    }
    fn xvb_run_actions(
        &mut self,
        ui: &mut Ui,
        height: f32,
        xvb_is_waiting: bool,
        xvb_is_alive: bool,
        key: &KeyPressed,
        wants_input: bool,
    ) {
        ui.group(|ui| {
            let width = (ui.available_width() / 3.0) - 5.0;
            let size = vec2(width, height);
            if xvb_is_waiting {
                ui.add_enabled_ui(false, |ui| {
                    ui.add_sized(size, Button::new("⟲"))
                        .on_disabled_hover_text(XVB_MIDDLE);
                    ui.add_sized(size, Button::new("⏹"))
                        .on_disabled_hover_text(XVB_MIDDLE);
                    ui.add_sized(size, Button::new("▶"))
                        .on_disabled_hover_text(XVB_MIDDLE);
                });
            } else if xvb_is_alive {
                if key.is_up() && !wants_input
                    || ui
                        .add_sized(size, Button::new("⟲"))
                        .on_hover_text("Restart Xvb")
                        .clicked()
                {
                    Helper::restart_xvb(
                        &self.helper,
                        &self.state.xvb,
                        &self.state.p2pool,
                        &self.state.xmrig,
                    );
                }
                if key.is_down() && !wants_input
                    || ui
                        .add_sized(size, Button::new("⏹"))
                        .on_hover_text("Stop Xvb")
                        .clicked()
                {
                    Helper::stop_xvb(&self.helper);
                }
                ui.add_enabled_ui(false, |ui| {
                    ui.add_sized(size, Button::new("▶"))
                        .on_disabled_hover_text("Start Xvb");
                });
            } else {
                ui.add_enabled_ui(false, |ui| {
                    ui.add_sized(size, Button::new("⟲"))
                        .on_disabled_hover_text("Restart Xvb");
                    ui.add_sized(size, Button::new("⏹"))
                        .on_disabled_hover_text("Stop Xvb");
                });
                // verify that address and token syntaxes are correct
                let ui_enabled = Regexes::addr_ok(&self.state.p2pool.address)
                    && self.state.xvb.token.len() == 9
                    && self.state.xvb.token.parse::<u32>().is_ok();
                ui.set_enabled(ui_enabled);
                let color = if ui_enabled { GREEN } else { RED };
                if (ui_enabled && key.is_up() && !wants_input)
                    || ui
                        .add_sized(size, Button::new(RichText::new("▶").color(color)))
                        .on_hover_text("Start Xvb")
                        .on_disabled_hover_text(XVB_NOT_CONFIGURED)
                        .clicked()
                {
                    Helper::start_xvb(
                        &self.helper,
                        &self.state.xvb,
                        &self.state.p2pool,
                        &self.state.xmrig,
                    );
                }
            }
        });
    }
}

fn status_p2pool(state: ProcessState, ui: &mut Ui, size: Vec2) {
    let color;
    let hover_text = match state {
        Alive => {
            color = GREEN;
            P2POOL_ALIVE
        }
        Dead => {
            color = GRAY;
            P2POOL_DEAD
        }
        Failed => {
            color = RED;
            P2POOL_FAILED
        }
        Syncing => {
            color = ORANGE;
            P2POOL_SYNCING
        }
        Middle | Waiting | NotMining | OfflineNodesAll | Retry => {
            color = YELLOW;
            P2POOL_MIDDLE
        }
    };
    status(ui, color, hover_text, size, "P2pool  ⏺");
}

fn status_xmrig(state: ProcessState, ui: &mut Ui, size: Vec2) {
    let color;
    let hover_text = match state {
        Alive => {
            color = GREEN;
            XMRIG_ALIVE
        }
        Dead => {
            color = GRAY;
            XMRIG_DEAD
        }
        Failed => {
            color = RED;
            XMRIG_FAILED
        }
        NotMining | OfflineNodesAll => {
            color = ORANGE;
            XMRIG_NOT_MINING
        }
        Middle | Waiting | Syncing | Retry => {
            color = YELLOW;
            XMRIG_MIDDLE
        }
    };
    status(ui, color, hover_text, size, "XMRig  ⏺");
}
fn status_xvb(state: ProcessState, ui: &mut Ui, size: Vec2) {
    let color;
    let hover_text = match state {
        Alive => {
            color = GREEN;
            XVB_ALIVE
        }
        Dead => {
            color = GRAY;
            XVB_DEAD
        }
        Failed | Retry => {
            color = RED;
            XVB_FAILED
        }
        NotMining | Syncing | OfflineNodesAll => {
            color = ORANGE;
            XVB_PUBLIC_ONLY
        }
        Middle | Waiting => {
            color = YELLOW;
            XVB_MIDDLE
        }
    };
    status(ui, color, hover_text, size, "XvB  ⏺");
}

fn status(ui: &mut Ui, color: Color32, hover_text: &str, _size: Vec2, text: &str) {
    ui.label(RichText::new(text).color(color))
        .on_hover_text(hover_text);
}
