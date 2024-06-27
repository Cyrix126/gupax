use crate::cli::parse_args;
use crate::cli::Cli;
use crate::components::gupax::FileWindow;
use crate::components::node::Ping;
use crate::components::node::RemoteNode;
use crate::components::node::REMOTE_NODES;
use crate::components::update::Update;
use crate::disk::consts::NODE_TOML;
use crate::disk::consts::POOL_TOML;
use crate::disk::consts::STATE_TOML;
use crate::disk::get_gupax_data_path;
use crate::disk::gupax_p2pool_api::GupaxP2poolApi;
use crate::disk::node::Node;
use crate::disk::pool::Pool;
use crate::disk::state::State;
use crate::errors::ErrorButtons;
use crate::errors::ErrorFerris;
use crate::errors::ErrorState;
use crate::helper::p2pool::ImgP2pool;
use crate::helper::p2pool::PubP2poolApi;
use crate::helper::xmrig::ImgXmrig;
use crate::helper::xmrig::PubXmrigApi;
use crate::helper::xvb::PubXvbApi;
use crate::helper::Helper;
use crate::helper::Process;
use crate::helper::ProcessName;
use crate::helper::Sys;
use crate::inits::init_text_styles;
use crate::miscs::cmp_f64;
use crate::miscs::get_exe;
use crate::miscs::get_exe_dir;
use crate::utils::constants::VISUALS;
use crate::utils::macros::arc_mut;
use crate::utils::macros::lock;
use crate::utils::sudo::SudoState;
use crate::APP_DEFAULT_HEIGHT;
use crate::APP_DEFAULT_WIDTH;
use crate::GUPAX_VERSION;
use crate::OS;
use eframe::CreationContext;
use egui::vec2;
use egui::Vec2;
use log::debug;
use log::error;
use log::info;
use log::warn;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

pub mod eframe_impl;
pub mod keys;
pub mod panels;
pub mod quit;
pub mod resize;
//---------------------------------------------------------------------------------------------------- Struct + Impl
// The state of the outer main [App].
// See the [State] struct in [state.rs] for the
// actual inner state of the tab settings.
#[allow(dead_code)]
pub struct App {
    // Misc state
    pub tab: Tab,   // What tab are we on?
    pub size: Vec2, // Top-level width and Top-level height
    // Alpha (transparency)
    // This value is used to incrementally increase/decrease
    // the transparency when resizing. Basically, it fades
    // in/out of black to hide jitter when resizing with [init_text_styles()]
    pub alpha: u8,
    // This is a one time trigger so [init_text_styles()] isn't
    // called 60x a second when resizing the window. Instead,
    // it only gets called if this bool is true and the user
    // is hovering over egui (ctx.is_pointer_over_area()).
    pub must_resize: bool, // Sets the flag so we know to [init_text_styles()]
    pub resizing: bool,    // Are we in the process of resizing? (For black fade in/out)
    // State
    pub og: Arc<Mutex<State>>,      // og = Old state to compare against
    pub state: State,               // state = Working state (current settings)
    pub update: Arc<Mutex<Update>>, // State for update data [update.rs]
    pub file_window: Arc<Mutex<FileWindow>>, // State for the path selector in [Gupax]
    pub ping: Arc<Mutex<Ping>>,     // Ping data found in [node.rs]
    pub og_node_vec: Vec<(String, Node)>, // Manual Node database
    pub node_vec: Vec<(String, Node)>, // Manual Node database
    pub og_pool_vec: Vec<(String, Pool)>, // Manual Pool database
    pub pool_vec: Vec<(String, Pool)>, // Manual Pool database
    pub diff: bool,                 // This bool indicates state changes
    // Restart state:
    // If Gupax updated itself, this represents that the
    // user should (but isn't required to) restart Gupax.
    pub restart: Arc<Mutex<Restart>>,
    // Error State:
    // These values are essentially global variables that
    // indicate if an error message needs to be displayed
    // (it takes up the whole screen with [error_msg] and buttons for ok/quit/etc)
    pub error_state: ErrorState,
    // Helper/API State:
    // This holds everything related to the data processed by the "helper thread".
    // This includes the "helper" threads public P2Pool/XMRig's API.
    pub helper: Arc<Mutex<Helper>>, // [Helper] state, mostly for Gupax uptime
    pub pub_sys: Arc<Mutex<Sys>>,   // [Sys] state, read by [Status], mutated by [Helper]
    pub p2pool: Arc<Mutex<Process>>, // [P2Pool] process state
    pub xmrig: Arc<Mutex<Process>>, // [XMRig] process state
    pub xvb: Arc<Mutex<Process>>,   // [Xvb] process state
    pub p2pool_api: Arc<Mutex<PubP2poolApi>>, // Public ready-to-print P2Pool API made by the "helper" thread
    pub xmrig_api: Arc<Mutex<PubXmrigApi>>, // Public ready-to-print XMRig API made by the "helper" thread
    pub xvb_api: Arc<Mutex<PubXvbApi>>,     // Public XvB API
    pub p2pool_img: Arc<Mutex<ImgP2pool>>,  // A one-time snapshot of what data P2Pool started with
    pub xmrig_img: Arc<Mutex<ImgXmrig>>,    // A one-time snapshot of what data XMRig started with
    // STDIN Buffer
    pub p2pool_stdin: String, // The buffer between the p2pool console and the [Helper]
    pub xmrig_stdin: String,  // The buffer between the xmrig console and the [Helper]
    // Sudo State
    pub sudo: Arc<Mutex<SudoState>>, // This is just a dummy struct on [Windows].
    // State from [--flags]
    pub no_startup: bool,
    // Gupax-P2Pool API
    // Gupax's P2Pool API (e.g: ~/.local/share/gupax/p2pool/)
    // This is a file-based API that contains data for permanent stats.
    // The below struct holds everything needed for it, the paths, the
    // actual stats, and all the functions needed to mutate them.
    pub gupax_p2pool_api: Arc<Mutex<GupaxP2poolApi>>,
    // Static stuff
    pub benchmarks: Vec<Benchmark>,     // XMRig CPU benchmarks
    pub pid: sysinfo::Pid,              // Gupax's PID
    pub max_threads: usize,             // Max amount of detected system threads
    pub now: Instant,                   // Internal timer
    pub exe: String,                    // Path for [Gupax] binary
    pub dir: String,                    // Directory [Gupax] binary is in
    pub resolution: Vec2,               // Frame resolution
    pub os: &'static str,               // OS
    pub admin: bool,                    // Are we admin? (for Windows)
    pub os_data_path: PathBuf,          // OS data path (e.g: ~/.local/share/gupax/)
    pub gupax_p2pool_api_path: PathBuf, // Gupax-P2Pool API path (e.g: ~/.local/share/gupax/p2pool/)
    pub state_path: PathBuf,            // State file path
    pub node_path: PathBuf,             // Node file path
    pub pool_path: PathBuf,             // Pool file path
    pub version: &'static str,          // Gupax version
    pub name_version: String,           // [Gupax vX.X.X]
}

impl App {
    #[cold]
    #[inline(never)]
    pub fn cc(cc: &CreationContext<'_>, resolution: Vec2, app: Self) -> Self {
        init_text_styles(
            &cc.egui_ctx,
            resolution[0],
            crate::miscs::clamp_scale(app.state.gupax.selected_scale),
        );
        cc.egui_ctx.set_visuals(VISUALS.clone());
        Self { resolution, ..app }
    }

    #[cold]
    #[inline(never)]
    pub fn save_before_quit(&mut self) {
        if let Err(e) = State::save(&mut self.state, &self.state_path) {
            error!("State file: {}", e);
        }
        if let Err(e) = Node::save(&self.node_vec, &self.node_path) {
            error!("Node list: {}", e);
        }
        if let Err(e) = Pool::save(&self.pool_vec, &self.pool_path) {
            error!("Pool list: {}", e);
        }
    }

    #[cold]
    #[inline(never)]
    pub fn new(now: Instant, args: Cli) -> Self {
        info!("Initializing App Struct...");
        info!("App Init | P2Pool & XMRig processes...");
        let p2pool = arc_mut!(Process::new(
            ProcessName::P2pool,
            String::new(),
            PathBuf::new()
        ));
        let xmrig = arc_mut!(Process::new(
            ProcessName::Xmrig,
            String::new(),
            PathBuf::new()
        ));
        let xvb = arc_mut!(Process::new(
            ProcessName::Xvb,
            String::new(),
            PathBuf::new()
        ));
        let p2pool_api = arc_mut!(PubP2poolApi::new());
        let xmrig_api = arc_mut!(PubXmrigApi::new());
        let xvb_api = arc_mut!(PubXvbApi::new());
        let p2pool_img = arc_mut!(ImgP2pool::new());
        let xmrig_img = arc_mut!(ImgXmrig::new());

        info!("App Init | Sysinfo...");
        // We give this to the [Helper] thread.
        let mut sysinfo = sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::new()
                .with_cpu(sysinfo::CpuRefreshKind::everything())
                .with_processes(sysinfo::ProcessRefreshKind::new().with_cpu())
                .with_memory(sysinfo::MemoryRefreshKind::everything()),
        );
        sysinfo.refresh_all();
        let pid = match sysinfo::get_current_pid() {
            Ok(pid) => pid,
            Err(e) => {
                error!("App Init | Failed to get sysinfo PID: {}", e);
                exit(1)
            }
        };
        let pub_sys = arc_mut!(Sys::new());

        // CPU Benchmark data initialization.
        info!("App Init | Initializing CPU benchmarks...");
        let benchmarks: Vec<Benchmark> = {
            let cpu = sysinfo.cpus()[0].brand();
            let mut json: Vec<Benchmark> =
                serde_json::from_slice(include_bytes!("../../assets/cpu.json")).unwrap();
            json.sort_by(|a, b| cmp_f64(strsim::jaro(&b.cpu, cpu), strsim::jaro(&a.cpu, cpu)));
            json
        };
        info!("App Init | Assuming user's CPU is: {}", benchmarks[0].cpu);

        info!("App Init | The rest of the [App]...");
        let mut app = Self {
            tab: Tab::default(),
            ping: arc_mut!(Ping::new()),
            size: vec2(APP_DEFAULT_WIDTH, APP_DEFAULT_HEIGHT),
            must_resize: false,
            og: arc_mut!(State::new()),
            state: State::new(),
            update: arc_mut!(Update::new(String::new(), PathBuf::new(), PathBuf::new(),)),
            file_window: FileWindow::new(),
            og_node_vec: Node::new_vec(),
            node_vec: Node::new_vec(),
            og_pool_vec: Pool::new_vec(),
            pool_vec: Pool::new_vec(),
            restart: arc_mut!(Restart::No),
            diff: false,
            error_state: ErrorState::new(),
            helper: arc_mut!(Helper::new(
                now,
                pub_sys.clone(),
                p2pool.clone(),
                xmrig.clone(),
                xvb.clone(),
                p2pool_api.clone(),
                xmrig_api.clone(),
                xvb_api.clone(),
                p2pool_img.clone(),
                xmrig_img.clone(),
                arc_mut!(GupaxP2poolApi::new())
            )),
            p2pool,
            xmrig,
            xvb,
            p2pool_api,
            xvb_api,
            xmrig_api,
            p2pool_img,
            xmrig_img,
            p2pool_stdin: String::with_capacity(10),
            xmrig_stdin: String::with_capacity(10),
            sudo: arc_mut!(SudoState::new()),
            resizing: false,
            alpha: 0,
            no_startup: false,
            gupax_p2pool_api: arc_mut!(GupaxP2poolApi::new()),
            pub_sys,
            benchmarks,
            pid,
            max_threads: benri::threads!(),
            now,
            admin: false,
            exe: String::new(),
            dir: String::new(),
            resolution: Vec2::new(APP_DEFAULT_HEIGHT, APP_DEFAULT_WIDTH),
            os: OS,
            os_data_path: PathBuf::new(),
            gupax_p2pool_api_path: PathBuf::new(),
            state_path: PathBuf::new(),
            node_path: PathBuf::new(),
            pool_path: PathBuf::new(),
            version: GUPAX_VERSION,
            name_version: format!("Gupaxx {}", GUPAX_VERSION),
        };
        //---------------------------------------------------------------------------------------------------- App init data that *could* panic
        info!("App Init | Getting EXE path...");
        let mut panic = String::new();
        // Get exe path
        app.exe = match get_exe() {
            Ok(exe) => exe,
            Err(e) => {
                panic = format!("get_exe(): {}", e);
                app.error_state
                    .set(panic.clone(), ErrorFerris::Panic, ErrorButtons::Quit);
                String::new()
            }
        };
        // Get exe directory path
        app.dir = match get_exe_dir() {
            Ok(dir) => dir,
            Err(e) => {
                panic = format!("get_exe_dir(): {}", e);
                app.error_state
                    .set(panic.clone(), ErrorFerris::Panic, ErrorButtons::Quit);
                String::new()
            }
        };
        // Get OS data path
        app.os_data_path = match get_gupax_data_path() {
            Ok(dir) => dir,
            Err(e) => {
                panic = format!("get_os_data_path(): {}", e);
                app.error_state
                    .set(panic.clone(), ErrorFerris::Panic, ErrorButtons::Quit);
                PathBuf::new()
            }
        };

        info!("App Init | Setting TOML path...");
        // Set [*.toml] path
        app.state_path.clone_from(&app.os_data_path);
        app.state_path.push(STATE_TOML);
        app.node_path.clone_from(&app.os_data_path);
        app.node_path.push(NODE_TOML);
        app.pool_path.clone_from(&app.os_data_path);
        app.pool_path.push(POOL_TOML);
        // Set GupaxP2poolApi path
        app.gupax_p2pool_api_path = crate::disk::get_gupax_p2pool_path(&app.os_data_path);
        lock!(app.gupax_p2pool_api).fill_paths(&app.gupax_p2pool_api_path);

        // Apply arg state
        // It's not safe to [--reset] if any of the previous variables
        // are unset (null path), so make sure we just abort if the [panic] String contains something.
        info!("App Init | Applying argument state...");
        let mut app = parse_args(app, args, panic);

        use crate::disk::errors::TomlError::*;
        // Read disk state
        info!("App Init | Reading disk state...");
        app.state = match State::get(&app.state_path) {
            Ok(toml) => toml,
            Err(err) => {
                error!("State ... {}", err);
                let set = match err {
                    Io(e) => Some((e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit)),
                    Path(e) => Some((e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit)),
                    Serialize(e) => Some((e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit)),
                    Deserialize(e) => Some((e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit)),
                    Format(e) => Some((e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit)),
                    Merge(e) => Some((e.to_string(), ErrorFerris::Error, ErrorButtons::ResetState)),
                    _ => None,
                };
                if let Some((e, ferris, button)) = set {
                    app.error_state.set(format!("State file: {}\n\nTry deleting: {}\n\n(Warning: this will delete your Gupax settings)\n\n", e, app.state_path.display()), ferris, button);
                }

                State::new()
            }
        };
        // Clamp window resolution scaling values.
        app.state.gupax.selected_scale = crate::miscs::clamp_scale(app.state.gupax.selected_scale);

        app.og = arc_mut!(app.state.clone());
        // Read node list
        info!("App Init | Reading node list...");
        app.node_vec = match Node::get(&app.node_path) {
            Ok(toml) => toml,
            Err(err) => {
                error!("Node ... {}", err);
                let (e, ferris, button) = match err {
                    Io(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Path(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Serialize(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Deserialize(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Format(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Merge(e) => (e.to_string(), ErrorFerris::Error, ErrorButtons::ResetState),
                    Parse(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                };
                app.error_state.set(format!("Node list: {}\n\nTry deleting: {}\n\n(Warning: this will delete your custom node list)\n\n", e, app.node_path.display()), ferris, button);
                Node::new_vec()
            }
        };
        app.og_node_vec.clone_from(&app.node_vec);
        debug!("Node Vec:");
        debug!("{:#?}", app.node_vec);
        // Read pool list
        info!("App Init | Reading pool list...");
        app.pool_vec = match Pool::get(&app.pool_path) {
            Ok(toml) => toml,
            Err(err) => {
                error!("Pool ... {}", err);
                let (e, ferris, button) = match err {
                    Io(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Path(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Serialize(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Deserialize(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Format(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Merge(e) => (e.to_string(), ErrorFerris::Error, ErrorButtons::ResetState),
                    Parse(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                };
                app.error_state.set(format!("Pool list: {}\n\nTry deleting: {}\n\n(Warning: this will delete your custom pool list)\n\n", e, app.pool_path.display()), ferris, button);
                Pool::new_vec()
            }
        };
        app.og_pool_vec.clone_from(&app.pool_vec);
        debug!("Pool Vec:");
        debug!("{:#?}", app.pool_vec);

        //----------------------------------------------------------------------------------------------------
        // Read [GupaxP2poolApi] disk files
        let mut gupax_p2pool_api = lock!(app.gupax_p2pool_api);
        match GupaxP2poolApi::create_all_files(&app.gupax_p2pool_api_path) {
            Ok(_) => info!("App Init | Creating Gupax-P2Pool API files ... OK"),
            Err(err) => {
                error!("GupaxP2poolApi ... {}", err);
                let (e, ferris, button) = match err {
                    Io(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Path(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Serialize(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Deserialize(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Format(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Merge(e) => (e.to_string(), ErrorFerris::Error, ErrorButtons::ResetState),
                    Parse(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                };
                app.error_state.set(format!("Gupaxx P2Pool Stats: {}\n\nTry deleting: {}\n\n(Warning: this will delete your P2Pool payout history...!)\n\n", e, app.gupax_p2pool_api_path.display()), ferris, button);
            }
        }
        info!("App Init | Reading Gupax-P2Pool API files...");
        match gupax_p2pool_api.read_all_files_and_update() {
            Ok(_) => {
                info!(
                    "GupaxP2poolApi ... Payouts: {} | XMR (atomic-units): {}",
                    gupax_p2pool_api.payout, gupax_p2pool_api.xmr,
                );
            }
            Err(err) => {
                error!("GupaxP2poolApi ... {}", err);
                let (e, ferris, button) = match err {
                    Io(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Path(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Serialize(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Deserialize(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Format(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                    Merge(e) => (e.to_string(), ErrorFerris::Error, ErrorButtons::ResetState),
                    Parse(e) => (e.to_string(), ErrorFerris::Panic, ErrorButtons::Quit),
                };
                app.error_state.set(format!("Gupaxx P2Pool Stats: {}\n\nTry deleting: {}\n\n(Warning: this will delete your P2Pool payout history...!)\n\n", e, app.gupax_p2pool_api_path.display()), ferris, button);
            }
        };
        drop(gupax_p2pool_api);
        lock!(app.helper).gupax_p2pool_api = Arc::clone(&app.gupax_p2pool_api);

        //----------------------------------------------------------------------------------------------------
        let mut og = lock!(app.og); // Lock [og]
                                    // Handle max threads
        info!("App Init | Handling max thread overflow...");
        og.xmrig.max_threads = app.max_threads;
        let current = og.xmrig.current_threads;
        let max = og.xmrig.max_threads;
        if current > max {
            og.xmrig.current_threads = max;
        }
        // Handle [node_vec] overflow
        info!("App Init | Handling [node_vec] overflow");
        if og.p2pool.selected_index > app.og_node_vec.len() {
            warn!(
                "App | Overflowing manual node index [{} > {}]",
                og.p2pool.selected_index,
                app.og_node_vec.len()
            );
            let (name, node) = match app.og_node_vec.first() {
                Some(zero) => zero.clone(),
                None => Node::new_tuple(),
            };
            og.p2pool.selected_index = 0;
            og.p2pool.selected_name.clone_from(&name);
            og.p2pool.selected_ip.clone_from(&node.ip);
            og.p2pool.selected_rpc.clone_from(&node.rpc);
            og.p2pool.selected_zmq.clone_from(&node.zmq);
            app.state.p2pool.selected_index = 0;
            app.state.p2pool.selected_name = name;
            app.state.p2pool.selected_ip = node.ip;
            app.state.p2pool.selected_rpc = node.rpc;
            app.state.p2pool.selected_zmq = node.zmq;
        }
        // Handle [pool_vec] overflow
        info!("App Init | Handling [pool_vec] overflow...");
        if og.xmrig.selected_index > app.og_pool_vec.len() {
            warn!(
                "App | Overflowing manual pool index [{} > {}], resetting to 1",
                og.xmrig.selected_index,
                app.og_pool_vec.len()
            );
            let (name, pool) = match app.og_pool_vec.first() {
                Some(zero) => zero.clone(),
                None => Pool::new_tuple(),
            };
            og.xmrig.selected_index = 0;
            og.xmrig.selected_name.clone_from(&name);
            og.xmrig.selected_ip.clone_from(&pool.ip);
            og.xmrig.selected_port.clone_from(&pool.port);
            app.state.xmrig.selected_index = 0;
            app.state.xmrig.selected_name = name;
            app.state.xmrig.selected_ip = pool.ip;
            app.state.xmrig.selected_port = pool.port;
        }

        // Apply TOML values to [Update]
        info!("App Init | Applying TOML values to [Update]...");
        let p2pool_path = og.gupax.absolute_p2pool_path.clone();
        let xmrig_path = og.gupax.absolute_xmrig_path.clone();
        app.update = arc_mut!(Update::new(app.exe.clone(), p2pool_path, xmrig_path));

        // Set state version as compiled in version
        info!("App Init | Setting state Gupax version...");
        lock!(og.version).gupax = GUPAX_VERSION.to_string();
        lock!(app.state.version).gupax = GUPAX_VERSION.to_string();

        // Set saved [Tab]
        info!("App Init | Setting saved [Tab]...");
        app.tab = app.state.gupax.tab;

        // Set saved Hero mode to runtime.
        debug!("Setting runtime_mode & runtime_manual_amount");
        app.xvb_api.lock().unwrap().stats_priv.runtime_mode = app.state.xvb.mode.clone().into();
        app.xvb_api.lock().unwrap().stats_priv.runtime_manual_amount =
            app.state.xvb.manual_amount_raw;
        // Check if [P2pool.node] exists
        info!("App Init | Checking if saved remote node still exists...");
        app.state.p2pool.node = RemoteNode::check_exists(&app.state.p2pool.node);

        drop(og); // Unlock [og]

        // Spawn the "Helper" thread.
        info!("Helper | Spawning helper thread...");
        Helper::spawn_helper(&app.helper, sysinfo, app.pid, app.max_threads);
        info!("Helper ... OK");

        // Check for privilege. Should be Admin on [Windows] and NOT root on Unix.
        info!("App Init | Checking for privilege level...");
        #[cfg(target_os = "windows")]
        if is_elevated::is_elevated() {
            app.admin = true;
        } else {
            error!("Windows | Admin user not detected!");
            app.error_state.set(format!("Gupaxx was not launched as Administrator!\nBe warned, XMRig might have less hashrate!"), ErrorFerris::Sudo, ErrorButtons::WindowsAdmin);
        }
        #[cfg(target_family = "unix")]
        if sudo_check::check() != sudo_check::RunningAs::User {
            let id = sudo_check::check();
            error!("Unix | Regular user not detected: [{:?}]", id);
            app.error_state.set(format!("Gupaxx was launched as: [{:?}]\nPlease launch Gupax with regular user permissions.", id), ErrorFerris::Panic, ErrorButtons::Quit);
        }

        // macOS re-locates "dangerous" applications into some read-only "/private" directory.
        // It _seems_ to be fixed by moving [Gupax.app] into "/Applications".
        // So, detect if we are in in "/private" and warn the user.
        #[cfg(target_os = "macos")]
        if app.exe.starts_with("/private") {
            app.error_state.set(format!("macOS thinks Gupax is a virus!\n(macOS has relocated Gupax for security reasons)\n\nThe directory: [{}]\nSince this is a private read-only directory, it causes issues with updates and correctly locating P2Pool/XMRig. Please move Gupax into the [Applications] directory, this lets macOS relax a little.\n", app.exe), ErrorFerris::Panic, ErrorButtons::Quit);
        }

        info!("App ... OK");
        app
    }

    #[cold]
    #[inline(never)]
    pub fn gather_backup_hosts(&self) -> Option<Vec<Node>> {
        if !self.state.p2pool.backup_host {
            return None;
        }

        // INVARIANT:
        // We must ensure all nodes are capable of
        // sending/receiving valid JSON-RPC requests.
        //
        // This is done during the `Ping` phase, meaning
        // all the nodes listed in our `self.ping` should
        // have ping data. We can use this data to filter
        // out "dead" nodes.
        //
        // The user must have at least pinged once so that
        // we actually have this data to work off of, else,
        // this "backup host" feature will return here
        // with 0 extra nodes as we can't be sure that any
        // of them are actually online.
        //
        // Realistically, most of them are, but we can't be sure,
        // and checking here without explicitly asking the user
        // to connect to nodes is a no-go (also, non-async environment).
        if !lock!(self.ping).pinged {
            warn!("Backup hosts ... simple node backup: no ping data available, returning None");
            return None;
        }

        if self.state.p2pool.simple {
            let mut vec = Vec::with_capacity(REMOTE_NODES.len());

            // Locking during this entire loop should be fine,
            // only a few nodes to iter through.
            for pinged_node in lock!(self.ping).nodes.iter() {
                // Continue if this node is not green/yellow.
                if pinged_node.ms > crate::components::node::RED_NODE_PING {
                    continue;
                }

                let (ip, rpc, zmq) = RemoteNode::get_ip_rpc_zmq(pinged_node.ip);

                let node = Node {
                    ip: ip.into(),
                    rpc: rpc.into(),
                    zmq: zmq.into(),
                };

                vec.push(node);
            }

            if vec.is_empty() {
                warn!("Backup hosts ... simple node backup: no viable nodes found");
                None
            } else {
                info!("Backup hosts ... simple node backup list: {vec:#?}");
                Some(vec)
            }
        } else {
            Some(self.node_vec.iter().map(|(_, node)| node.clone()).collect())
        }
    }
}
//---------------------------------------------------------------------------------------------------- [Tab] Enum + Impl
// The tabs inside [App].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Tab {
    About,
    Status,
    Gupax,
    P2pool,
    Xmrig,
    Xvb,
}

impl Default for Tab {
    fn default() -> Self {
        Self::About
    }
}
//---------------------------------------------------------------------------------------------------- [Restart] Enum
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restart {
    No,  // We don't need to restart
    Yes, // We updated, user should probably (but isn't required to) restart
}
//---------------------------------------------------------------------------------------------------- CPU Benchmarks.
#[derive(Debug, Serialize, Deserialize)]
pub struct Benchmark {
    pub cpu: String,
    pub rank: u16,
    pub percent: f32,
    pub benchmarks: u16,
    pub average: f32,
    pub high: f32,
    pub low: f32,
}
#[cfg(test)]
mod test {
    use crate::miscs::cmp_f64;

    #[test]
    fn detect_benchmark_cpu() {
        use crate::app::Benchmark;
        let cpu = "AMD Ryzen 9 5950X 16-Core Processor";

        let benchmarks: Vec<Benchmark> = {
            let mut json: Vec<Benchmark> =
                serde_json::from_slice(include_bytes!("../../assets/cpu.json")).unwrap();
            json.sort_by(|a, b| cmp_f64(strsim::jaro(&b.cpu, cpu), strsim::jaro(&a.cpu, cpu)));
            json
        };

        assert!(benchmarks[0].cpu == "AMD Ryzen 9 5950X 16-Core Processor");
    }
}
