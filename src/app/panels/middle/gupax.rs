use crate::app::panels::middle::*;
use crate::app::ErrorState;
use crate::app::Restart;
use crate::components::gupax::*;
use crate::components::update::Update;
use crate::disk::state::*;
use crate::macros::lock2;
use log::debug;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
impl Gupax {
    #[inline(always)] // called once
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        og: &Arc<Mutex<State>>,
        state_path: &Path,
        update: &Arc<Mutex<Update>>,
        file_window: &Arc<Mutex<FileWindow>>,
        error_state: &mut ErrorState,
        restart: &Arc<Mutex<Restart>>,
        size: Vec2,
        _frame: &mut eframe::Frame,
        _ctx: &egui::Context,
        ui: &mut egui::Ui,
    ) {
        // Update button + Progress bar
        debug!("Gupaxx Tab | Rendering [Update] button + progress bar");
        ui.group(|ui| {
            let button = if self.simple {
                size.y / 5.0
            } else {
                size.y / 15.0
            };
            let height = if self.simple {
                size.y / 5.0
            } else {
                size.y / 10.0
            };
            let width = size.x - SPACE;
            let updating = *lock2!(update, updating);
            ui.vertical(|ui| {
                // If [Gupax] is being built for a Linux distro,
                // disable built-in updating completely.
                #[cfg(feature = "distro")]
                ui.set_enabled(false);
                #[cfg(feature = "distro")]
                ui.add_sized([width, button], Button::new("Updates are disabled"))
                    .on_disabled_hover_text(DISTRO_NO_UPDATE);
                #[cfg(not(feature = "distro"))]
                ui.set_enabled(!updating && *lock!(restart) == Restart::No);
                #[cfg(not(feature = "distro"))]
                if ui
                    .add_sized([width, button], Button::new("Check for updates"))
                    .on_hover_text(GUPAX_UPDATE)
                    .clicked()
                {
                    Update::spawn_thread(og, self, state_path, update, error_state, restart);
                }
            });
            ui.vertical(|ui| {
                ui.set_enabled(updating);
                let prog = *lock2!(update, prog);
                let msg = format!("{}\n{}{}", *lock2!(update, msg), prog, "%");
                ui.add_sized([width, height * 1.4], Label::new(RichText::new(msg)));
                let height = height / 2.0;
                let size = vec2(width, height);
                if updating {
                    ui.add_sized(size, Spinner::new().size(height));
                } else {
                    ui.add_sized(size, Label::new("..."));
                }
                ui.add_sized(size, ProgressBar::new(lock2!(update, prog).round() / 100.0));
            });
        });

        debug!("Gupaxx Tab | Rendering bool buttons");
        ui.horizontal(|ui| {
            ui.group(|ui| {
                let width = (size.x - SPACE * 15.0) / 7.0;
                let height = if self.simple {
                    size.y / 10.0
                } else {
                    size.y / 15.0
                };
                let size = vec2(width, height);
                ui.style_mut().override_text_style = Some(egui::TextStyle::Small);
                ui.add_sized(size, Checkbox::new(&mut self.auto_update, "Auto-Update"))
                    .on_hover_text(GUPAX_AUTO_UPDATE);
                ui.separator();
                ui.add_sized(size, Checkbox::new(&mut self.bundled, "Bundle"))
                    .on_hover_text(GUPAX_BUNDLED_UPDATE);
                ui.separator();
                ui.add_sized(size, Checkbox::new(&mut self.auto_p2pool, "Auto-P2Pool"))
                    .on_hover_text(GUPAX_AUTO_P2POOL);
                ui.separator();
                ui.add_sized(size, Checkbox::new(&mut self.auto_xmrig, "Auto-XMRig"))
                    .on_hover_text(GUPAX_AUTO_XMRIG);
                ui.separator();
                ui.add_sized(size, Checkbox::new(&mut self.auto_xvb, "Auto-XvB"))
                    .on_hover_text(GUPAX_AUTO_XVB);
                ui.separator();
                ui.add_sized(
                    size,
                    Checkbox::new(&mut self.ask_before_quit, "Confirm quit"),
                )
                .on_hover_text(GUPAX_ASK_BEFORE_QUIT);
                ui.separator();
                ui.add_sized(
                    size,
                    Checkbox::new(&mut self.save_before_quit, "Save on quit"),
                )
                .on_hover_text(GUPAX_SAVE_BEFORE_QUIT);
            });
        });

        if self.simple {
            return;
        }

        debug!("Gupaxx Tab | Rendering P2Pool/XMRig path selection");
        // P2Pool/XMRig binary path selection
        let height = size.y / 28.0;
        let text_edit = (ui.available_width() / 10.0) - SPACE;
        ui.group(|ui| {
            ui.add_sized(
                [ui.available_width(), height / 2.0],
                Label::new(
                    RichText::new("P2Pool/XMRig PATHs")
                        .underline()
                        .color(LIGHT_GRAY),
                ),
            )
            .on_hover_text("Gupaxx is online");
            ui.separator();
            ui.horizontal(|ui| {
                if self.p2pool_path.is_empty() {
                    ui.add_sized(
                        [text_edit, height],
                        Label::new(RichText::new("P2Pool Binary Path ➖").color(LIGHT_GRAY)),
                    )
                    .on_hover_text(P2POOL_PATH_EMPTY);
                } else if !Self::path_is_file(&self.p2pool_path) {
                    ui.add_sized(
                        [text_edit, height],
                        Label::new(RichText::new("P2Pool Binary Path ❌").color(RED)),
                    )
                    .on_hover_text(P2POOL_PATH_NOT_FILE);
                } else if !crate::components::update::check_p2pool_path(&self.p2pool_path) {
                    ui.add_sized(
                        [text_edit, height],
                        Label::new(RichText::new("P2Pool Binary Path ❌").color(RED)),
                    )
                    .on_hover_text(P2POOL_PATH_NOT_VALID);
                } else {
                    ui.add_sized(
                        [text_edit, height],
                        Label::new(RichText::new("P2Pool Binary Path ✔").color(GREEN)),
                    )
                    .on_hover_text(P2POOL_PATH_OK);
                }
                ui.spacing_mut().text_edit_width = ui.available_width() - SPACE;
                ui.set_enabled(!lock!(file_window).thread);
                if ui.button("Open").on_hover_text(GUPAX_SELECT).clicked() {
                    Self::spawn_file_window_thread(file_window, FileType::P2pool);
                }
                ui.add_sized(
                    [ui.available_width(), height],
                    TextEdit::singleline(&mut self.p2pool_path),
                )
                .on_hover_text(GUPAX_PATH_P2POOL);
            });
            ui.horizontal(|ui| {
                if self.xmrig_path.is_empty() {
                    ui.add_sized(
                        [text_edit, height],
                        Label::new(RichText::new(" XMRig Binary Path ➖").color(LIGHT_GRAY)),
                    )
                    .on_hover_text(XMRIG_PATH_EMPTY);
                } else if !Self::path_is_file(&self.xmrig_path) {
                    ui.add_sized(
                        [text_edit, height],
                        Label::new(RichText::new(" XMRig Binary Path ❌").color(RED)),
                    )
                    .on_hover_text(XMRIG_PATH_NOT_FILE);
                } else if !crate::components::update::check_xmrig_path(&self.xmrig_path) {
                    ui.add_sized(
                        [text_edit, height],
                        Label::new(RichText::new(" XMRig Binary Path ❌").color(RED)),
                    )
                    .on_hover_text(XMRIG_PATH_NOT_VALID);
                } else {
                    ui.add_sized(
                        [text_edit, height],
                        Label::new(RichText::new(" XMRig Binary Path ✔").color(GREEN)),
                    )
                    .on_hover_text(XMRIG_PATH_OK);
                }
                ui.spacing_mut().text_edit_width = ui.available_width() - SPACE;
                ui.set_enabled(!lock!(file_window).thread);
                if ui.button("Open").on_hover_text(GUPAX_SELECT).clicked() {
                    Self::spawn_file_window_thread(file_window, FileType::Xmrig);
                }
                ui.add_sized(
                    [ui.available_width(), height],
                    TextEdit::singleline(&mut self.xmrig_path),
                )
                .on_hover_text(GUPAX_PATH_XMRIG);
            });
        });
        let mut guard = lock!(file_window);
        if guard.picked_p2pool {
            self.p2pool_path.clone_from(&guard.p2pool_path);
            guard.picked_p2pool = false;
        }
        if guard.picked_xmrig {
            self.xmrig_path.clone_from(&guard.xmrig_path);
            guard.picked_xmrig = false;
        }
        drop(guard);

        let height = ui.available_height() / 6.0;

        // Saved [Tab]
        debug!("Gupaxx Tab | Rendering [Tab] selector");
        ui.group(|ui| {
            let width = (size.x / 6.0) - (SPACE * 1.93);
            let size = vec2(width, height);
            ui.add_sized(
                [ui.available_width(), height / 2.0],
                Label::new(RichText::new("Default Tab").underline().color(LIGHT_GRAY)),
            )
            .on_hover_text(GUPAX_TAB);
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_sized(size, SelectableLabel::new(self.tab == Tab::About, "About"))
                    .on_hover_text(GUPAX_TAB_ABOUT)
                    .clicked()
                {
                    self.tab = Tab::About;
                }
                ui.separator();
                if ui
                    .add_sized(
                        size,
                        SelectableLabel::new(self.tab == Tab::Status, "Status"),
                    )
                    .on_hover_text(GUPAX_TAB_STATUS)
                    .clicked()
                {
                    self.tab = Tab::Status;
                }
                ui.separator();
                if ui
                    .add_sized(size, SelectableLabel::new(self.tab == Tab::Gupax, "Gupaxx"))
                    .on_hover_text(GUPAX_TAB_GUPAX)
                    .clicked()
                {
                    self.tab = Tab::Gupax;
                }
                ui.separator();
                if ui
                    .add_sized(
                        size,
                        SelectableLabel::new(self.tab == Tab::P2pool, "P2Pool"),
                    )
                    .on_hover_text(GUPAX_TAB_P2POOL)
                    .clicked()
                {
                    self.tab = Tab::P2pool;
                }
                ui.separator();
                if ui
                    .add_sized(size, SelectableLabel::new(self.tab == Tab::Xmrig, "XMRig"))
                    .on_hover_text(GUPAX_TAB_XMRIG)
                    .clicked()
                {
                    self.tab = Tab::Xmrig;
                }
                if ui
                    .add_sized(size, SelectableLabel::new(self.tab == Tab::Xvb, "XvB"))
                    .on_hover_text(GUPAX_TAB_XVB)
                    .clicked()
                {
                    self.tab = Tab::Xvb;
                }
            })
        });

        // Gupax App resolution sliders
        debug!("Gupaxx Tab | Rendering resolution sliders");
        ui.group(|ui| {
            ui.add_sized(
                [ui.available_width(), height / 2.0],
                Label::new(
                    RichText::new("Width/Height Adjust")
                        .underline()
                        .color(LIGHT_GRAY),
                ),
            )
            .on_hover_text(GUPAX_ADJUST);
            ui.separator();
            ui.vertical(|ui| {
                let width = size.x / 10.0;
                ui.spacing_mut().icon_width = width / 25.0;
                ui.spacing_mut().slider_width = width * 7.6;
                match self.ratio {
                    Ratio::None => (),
                    Ratio::Width => {
                        let width = self.selected_width as f64;
                        let height = (width / 1.333).round();
                        self.selected_height = height as u16;
                    }
                    Ratio::Height => {
                        let height = self.selected_height as f64;
                        let width = (height * 1.333).round();
                        self.selected_width = width as u16;
                    }
                }
                let height = height / 3.5;
                let size = vec2(width, height);
                ui.horizontal(|ui| {
                    ui.set_enabled(self.ratio != Ratio::Height);
                    ui.add_sized(
                        size,
                        Label::new(format!(
                            " Width [{}-{}]:",
                            APP_MIN_WIDTH as u16, APP_MAX_WIDTH as u16
                        )),
                    );
                    ui.add_sized(
                        size,
                        Slider::new(
                            &mut self.selected_width,
                            APP_MIN_WIDTH as u16..=APP_MAX_WIDTH as u16,
                        ),
                    )
                    .on_hover_text(GUPAX_WIDTH);
                });
                ui.horizontal(|ui| {
                    ui.set_enabled(self.ratio != Ratio::Width);
                    ui.add_sized(
                        size,
                        Label::new(format!(
                            "Height [{}-{}]:",
                            APP_MIN_HEIGHT as u16, APP_MAX_HEIGHT as u16
                        )),
                    );
                    ui.add_sized(
                        size,
                        Slider::new(
                            &mut self.selected_height,
                            APP_MIN_HEIGHT as u16..=APP_MAX_HEIGHT as u16,
                        ),
                    )
                    .on_hover_text(GUPAX_HEIGHT);
                });
                ui.horizontal(|ui| {
                    ui.add_sized(
                        size,
                        Label::new(format!("Scaling [{APP_MIN_SCALE}..{APP_MAX_SCALE}]:")),
                    );
                    ui.add_sized(
                        size,
                        Slider::new(&mut self.selected_scale, APP_MIN_SCALE..=APP_MAX_SCALE)
                            .step_by(0.1),
                    )
                    .on_hover_text(GUPAX_SCALE);
                });
            });
            ui.style_mut().override_text_style = Some(egui::TextStyle::Button);
            ui.separator();
            // Width/Height locks
            ui.horizontal(|ui| {
                use Ratio::*;
                let width = (size.x / 4.0) - (SPACE * 1.5);
                let size = vec2(width, height);
                if ui
                    .add_sized(
                        size,
                        SelectableLabel::new(self.ratio == Width, "Lock to width"),
                    )
                    .on_hover_text(GUPAX_LOCK_WIDTH)
                    .clicked()
                {
                    self.ratio = Width;
                }
                ui.separator();
                if ui
                    .add_sized(
                        size,
                        SelectableLabel::new(self.ratio == Height, "Lock to height"),
                    )
                    .on_hover_text(GUPAX_LOCK_HEIGHT)
                    .clicked()
                {
                    self.ratio = Height;
                }
                ui.separator();
                if ui
                    .add_sized(size, SelectableLabel::new(self.ratio == None, "No lock"))
                    .on_hover_text(GUPAX_NO_LOCK)
                    .clicked()
                {
                    self.ratio = None;
                }
                if ui
                    .add_sized(size, Button::new("Set"))
                    .on_hover_text(GUPAX_SET)
                    .clicked()
                {
                    let size = Vec2::new(self.selected_width as f32, self.selected_height as f32);
                    ui.ctx()
                        .send_viewport_cmd(egui::viewport::ViewportCommand::InnerSize(size));
                }
            })
        });
    }
}
