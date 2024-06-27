use crate::app::keys::KeyPressed;
use crate::app::Tab;
use crate::helper::ProcessState;
use crate::utils::constants::*;
use crate::utils::errors::{ErrorButtons, ErrorFerris};
use crate::utils::macros::lock;
use egui::*;
use log::debug;

mod gupax;
mod p2pool;
mod status;
mod xmrig;
mod xvb;
impl crate::app::App {
    pub fn middle_panel(
        &mut self,
        ctx: &egui::Context,
        frame: &mut eframe::Frame,
        key: KeyPressed,
        p2pool_is_alive: bool,
        xmrig_is_alive: bool,
        xvb_is_alive: bool,
    ) {
        // Middle panel, contents of the [Tab]
        debug!("App | Rendering CENTRAL_PANEL (tab contents)");
        CentralPanel::default().show(ctx, |ui| {
			// This sets the Ui dimensions after Top/Bottom are filled
			self.size.x = ui.available_width();
			self.size.y = ui.available_height();
			ui.style_mut().override_text_style = Some(TextStyle::Body);
			match self.tab {
				Tab::About => {
					debug!("App | Entering [About] Tab");
					// If [D], show some debug info with [ErrorState]
					if key.is_d() {
						debug!("App | Entering [Debug Info]");
						#[cfg(feature = "distro")]
						let distro = true;
						#[cfg(not(feature = "distro"))]
						let distro = false;
						let p2pool_gui_len = lock!(self.p2pool_api).output.len();
						let xmrig_gui_len = lock!(self.xmrig_api).output.len();
						let gupax_p2pool_api = lock!(self.gupax_p2pool_api);
						let debug_info = format!(
"Gupax version: {}\n
Bundled P2Pool version: {}\n
Bundled XMRig version: {}\n
Gupax uptime: {} seconds\n
Selected resolution: {}x{}\n
Internal resolution: {}x{}\n
Operating system: {}\n
Max detected threads: {}\n
Gupax PID: {}\n
State diff: {}\n
Node list length: {}\n
Pool list length: {}\n
Admin privilege: {}\n
Release build: {}\n
Debug build: {}\n
Distro build: {}\n
Build commit: {}\n
OS Data PATH: {}\n
Gupax PATH: {}\n
P2Pool PATH: {}\n
XMRig PATH: {}\n
P2Pool console byte length: {}\n
XMRig console byte length: {}\n
------------------------------------------ P2POOL IMAGE ------------------------------------------
{:#?}\n
------------------------------------------ XMRIG IMAGE ------------------------------------------
{:#?}\n
------------------------------------------ GUPAX-P2POOL API ------------------------------------------
payout: {:#?}
payout_u64: {:#?}
xmr: {:#?}
path_log: {:#?}
path_payout: {:#?}
path_xmr: {:#?}\n
------------------------------------------ WORKING STATE ------------------------------------------
{:#?}\n
------------------------------------------ ORIGINAL STATE ------------------------------------------
{:#?}",
							GUPAX_VERSION,
							P2POOL_VERSION,
							XMRIG_VERSION,
							self.now.elapsed().as_secs_f32(),
							self.state.gupax.selected_width,
							self.state.gupax.selected_height,
							self.size.x,
							self.size.y,
							OS_NAME,
							self.max_threads,
							self.pid,
							self.diff,
							self.node_vec.len(),
							self.pool_vec.len(),
							self.admin,
							!cfg!(debug_assertions),
							cfg!(debug_assertions),
							distro,
							COMMIT,
							self.os_data_path.display(),
							self.exe,
							self.state.gupax.absolute_p2pool_path.display(),
							self.state.gupax.absolute_xmrig_path.display(),
							p2pool_gui_len,
							xmrig_gui_len,
							lock!(self.p2pool_img),
							lock!(self.xmrig_img),
							gupax_p2pool_api.payout,
							gupax_p2pool_api.payout_u64,
							gupax_p2pool_api.xmr,
							gupax_p2pool_api.path_log,
							gupax_p2pool_api.path_payout,
							gupax_p2pool_api.path_xmr,
							self.state,
							lock!(self.og),
						);
						self.error_state.set(debug_info, ErrorFerris::Cute, ErrorButtons::Debug);
					}
					let width = self.size.x;
					let height = self.size.y/30.0;
					let max_height = self.size.y;
					let size = vec2(width, height);
					ui.add_space(10.0);
					ui.vertical_centered(|ui| {
						ui.set_max_height(max_height);
						// Display [Gupax] banner
						let link_width = width/14.0;
                        ui.add_sized(Vec2::new(width, height*3.0), Image::from_bytes("bytes://banner.png", BYTES_BANNER));
						ui.add_sized(size, Label::new("is a GUI for mining"));
						ui.add_sized([link_width, height], Hyperlink::from_label_and_url("[Monero]", "https://www.github.com/monero-project/monero"));
						ui.add_sized(size, Label::new("on"));
						ui.add_sized([link_width, height], Hyperlink::from_label_and_url("[P2Pool]", "https://www.github.com/SChernykh/p2pool"));
						ui.add_sized(size, Label::new("using"));
						ui.add_sized([link_width, height], Hyperlink::from_label_and_url("[XMRig]", "https://www.github.com/xmrig/xmrig"));

						ui.add_space(SPACE*2.0);
						ui.add_sized(size, Label::new(KEYBOARD_SHORTCUTS));
						ui.add_space(SPACE*2.0);

						if cfg!(debug_assertions) { ui.label(format!("Gupax is running in debug mode - {}", self.now.elapsed().as_secs_f64())); }
						ui.label(format!("Gupax has been running for {}", lock!(self.pub_sys).gupax_uptime));
					});
				}
				Tab::Status => {
					debug!("App | Entering [Status] Tab");
					crate::disk::state::Status::show(&mut self.state.status, &self.pub_sys, &self.p2pool_api, &self.xmrig_api, &self.xvb_api,&self.p2pool_img, &self.xmrig_img, p2pool_is_alive, xmrig_is_alive,  xvb_is_alive, self.max_threads, &self.gupax_p2pool_api, &self.benchmarks, self.size, ctx, ui);
				}
				Tab::Gupax => {
					debug!("App | Entering [Gupax] Tab");
					crate::disk::state::Gupax::show(&mut self.state.gupax, &self.og, &self.state_path, &self.update, &self.file_window, &mut self.error_state, &self.restart, self.size,  frame, ctx, ui);
				}
				Tab::P2pool => {
					debug!("App | Entering [P2Pool] Tab");
					crate::disk::state::P2pool::show(&mut self.state.p2pool, &mut self.node_vec, &self.og, &self.ping, &self.p2pool, &self.p2pool_api, &mut self.p2pool_stdin, self.size, ctx, ui);
				}
				Tab::Xmrig => {
					debug!("App | Entering [XMRig] Tab");
					crate::disk::state::Xmrig::show(&mut self.state.xmrig, &mut self.pool_vec, &self.xmrig, &self.xmrig_api, &mut self.xmrig_stdin, self.size, ctx, ui);
				}
				Tab::Xvb => {
					debug!("App | Entering [XvB] Tab");
					crate::disk::state::Xvb::show(&mut self.state.xvb, self.size, &self.state.p2pool.address, ctx, ui, &self.xvb_api, &self.xmrig_api, lock!(self.xvb).state == ProcessState::Alive);
				}
			}
		});
    }
}
