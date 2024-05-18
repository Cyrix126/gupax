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

// This file represents the "helper" thread, which is the full separate thread
// that runs alongside the main [App] GUI thread. It exists for the entire duration
// of Gupax so that things can be handled without locking up the GUI thread.
//
// This thread is a continual 1 second loop, collecting available jobs on the
// way down and (if possible) asynchronously executing them at the very end.
//
// The main GUI thread will interface with this thread by mutating the Arc<Mutex>'s
// found here, e.g: User clicks [Stop P2Pool] -> Arc<Mutex<ProcessSignal> is set
// indicating to this thread during its loop: "I should stop P2Pool!", e.g:
//
//     if lock!(p2pool).signal == ProcessSignal::Stop {
//         stop_p2pool(),
//     }
//
// This also includes all things related to handling the child processes (P2Pool/XMRig):
// piping their stdout/stderr/stdin, accessing their APIs (HTTP + disk files), etc.

//---------------------------------------------------------------------------------------------------- Import
use crate::helper::{
    p2pool::{ImgP2pool, PubP2poolApi},
    xmrig::{ImgXmrig, PubXmrigApi},
};
use crate::{constants::*, disk::gupax_p2pool_api::GupaxP2poolApi, human::*, macros::*};
use log::*;
use std::path::Path;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::*,
};

use self::xvb::{nodes::XvbNode, PubXvbApi};
pub mod p2pool;
pub mod tests;
pub mod xmrig;
pub mod xvb;

//---------------------------------------------------------------------------------------------------- Constants
// The max amount of bytes of process output we are willing to
// hold in memory before it's too much and we need to reset.
const MAX_GUI_OUTPUT_BYTES: usize = 500_000;
// Just a little leeway so a reset will go off before the [String] allocates more memory.
const GUI_OUTPUT_LEEWAY: usize = MAX_GUI_OUTPUT_BYTES - 1000;

// Some constants for generating hashrate/difficulty.
const MONERO_BLOCK_TIME_IN_SECONDS: u64 = 120;
const P2POOL_BLOCK_TIME_IN_SECONDS: u64 = 10;

//---------------------------------------------------------------------------------------------------- [Helper] Struct
// A meta struct holding all the data that gets processed in this thread
pub struct Helper {
    pub instant: Instant,                             // Gupax start as an [Instant]
    pub uptime: HumanTime,                            // Gupax uptime formatting for humans
    pub pub_sys: Arc<Mutex<Sys>>, // The public API for [sysinfo] that the [Status] tab reads from
    pub p2pool: Arc<Mutex<Process>>, // P2Pool process state
    pub xmrig: Arc<Mutex<Process>>, // XMRig process state
    pub xvb: Arc<Mutex<Process>>, // XvB process state
    pub gui_api_p2pool: Arc<Mutex<PubP2poolApi>>, // P2Pool API state (for GUI thread)
    pub gui_api_xmrig: Arc<Mutex<PubXmrigApi>>, // XMRig API state (for GUI thread)
    pub gui_api_xvb: Arc<Mutex<PubXvbApi>>, // XMRig API state (for GUI thread)
    pub img_p2pool: Arc<Mutex<ImgP2pool>>, // A static "image" of the data P2Pool started with
    pub img_xmrig: Arc<Mutex<ImgXmrig>>, // A static "image" of the data XMRig started with
    pub_api_p2pool: Arc<Mutex<PubP2poolApi>>, // P2Pool API state (for Helper/P2Pool thread)
    pub_api_xmrig: Arc<Mutex<PubXmrigApi>>, // XMRig API state (for Helper/XMRig thread)
    pub_api_xvb: Arc<Mutex<PubXvbApi>>, // XvB API state (for Helper/XvB thread)
    pub gupax_p2pool_api: Arc<Mutex<GupaxP2poolApi>>, //
}

// The communication between the data here and the GUI thread goes as follows:
// [GUI] <---> [Helper] <---> [Watchdog] <---> [Private Data only available here]
//
// Both [GUI] and [Helper] own their separate [Pub*Api] structs.
// Since P2Pool & XMRig will be updating their information out of sync,
// it's the helpers job to lock everything, and move the watchdog [Pub*Api]s
// on a 1-second interval into the [GUI]'s [Pub*Api] struct, atomically.

//----------------------------------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct Sys {
    pub gupax_uptime: String,
    pub gupax_cpu_usage: String,
    pub gupax_memory_used_mb: String,
    pub system_cpu_model: String,
    pub system_memory: String,
    pub system_cpu_usage: String,
}

impl Sys {
    pub fn new() -> Self {
        Self {
            gupax_uptime: "0 seconds".to_string(),
            gupax_cpu_usage: "???%".to_string(),
            gupax_memory_used_mb: "??? megabytes".to_string(),
            system_cpu_usage: "???%".to_string(),
            system_memory: "???GB / ???GB".to_string(),
            system_cpu_model: "???".to_string(),
        }
    }
}
impl Default for Sys {
    fn default() -> Self {
        Self::new()
    }
}

//---------------------------------------------------------------------------------------------------- [Process] Struct
// This holds all the state of a (child) process.
// The main GUI thread will use this to display console text, online state, etc.
#[allow(dead_code)]
#[derive(Debug)]
pub struct Process {
    pub name: ProcessName,     // P2Pool or XMRig?
    pub state: ProcessState,   // The state of the process (alive, dead, etc)
    pub signal: ProcessSignal, // Did the user click [Start/Stop/Restart]?
    // STDIN Problem:
    //     - User can input many many commands in 1 second
    //     - The process loop only processes every 1 second
    //     - If there is only 1 [String] holding the user input,
    //       the user could overwrite their last input before
    //       the loop even has a chance to process their last command
    // STDIN Solution:
    //     - When the user inputs something, push it to a [Vec]
    //     - In the process loop, loop over every [Vec] element and
    //       send each one individually to the process stdin
    //
    pub input: Vec<String>,

    // The below are the handles to the actual child process.
    // [Simple] has no STDIN, but [Advanced] does. A PTY (pseudo-terminal) is
    // required for P2Pool/XMRig to open their STDIN pipe.
    //	child: Option<Arc<Mutex<Box<dyn portable_pty::Child + Send + std::marker::Sync>>>>, // STDOUT/STDERR is combined automatically thanks to this PTY, nice
    //	stdin: Option<Box<dyn portable_pty::MasterPty + Send>>, // A handle to the process's MasterPTY/STDIN

    // This is the process's private output [String], used by both [Simple] and [Advanced].
    // "parse" contains the output that will be parsed, then tossed out. "pub" will be written to
    // the same as parse, but it will be [swap()]'d by the "helper" thread into the GUIs [String].
    // The "helper" thread synchronizes this swap so that the data in here is moved there
    // roughly once a second. GUI thread never touches this.
    output_parse: Arc<Mutex<String>>,
    output_pub: Arc<Mutex<String>>,

    // Start time of process.
    start: std::time::Instant,
}

//---------------------------------------------------------------------------------------------------- [Process] Impl
impl Process {
    pub fn new(name: ProcessName, _args: String, _path: PathBuf) -> Self {
        Self {
            name,
            state: ProcessState::Dead,
            signal: ProcessSignal::None,
            start: Instant::now(),
            //			stdin: Option::None,
            //			child: Option::None,
            output_parse: arc_mut!(String::with_capacity(500)),
            output_pub: arc_mut!(String::with_capacity(500)),
            input: vec![String::new()],
        }
    }

    #[inline]
    // Convenience functions
    pub fn is_alive(&self) -> bool {
        self.state == ProcessState::Alive
            || self.state == ProcessState::Middle
            || self.state == ProcessState::Syncing
            || self.state == ProcessState::Retry
            || self.state == ProcessState::NotMining
            || self.state == ProcessState::OfflineNodesAll
    }

    #[inline]
    pub fn is_waiting(&self) -> bool {
        self.state == ProcessState::Middle || self.state == ProcessState::Waiting
    }
}

//---------------------------------------------------------------------------------------------------- [Process*] Enum
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ProcessState {
    Alive,   // Process is online, GREEN!
    Dead,    // Process is dead, BLACK!
    Failed,  // Process is dead AND exited with a bad code, RED!
    Middle,  // Process is in the middle of something ([re]starting/stopping), YELLOW!
    Waiting, // Process was successfully killed by a restart, and is ready to be started again, YELLOW!

    // Only for P2Pool and XvB, ORANGE.
    // XvB: Xmrig or P2pool are not alive
    Syncing,
    // XvB: if requests for stats fail, retry state to retry every minutes
    Retry,

    // Only for XMRig and XvB, ORANGE.
    // XvB: token or address are invalid even if syntax correct
    NotMining,
    // XvB: if node of XvB become unusable (ex: offline).
    OfflineNodesAll,
}

impl Default for ProcessState {
    fn default() -> Self {
        Self::Dead
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ProcessSignal {
    None,
    Start,
    Stop,
    Restart,
    UpdateNodes(XvbNode),
}

impl Default for ProcessSignal {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ProcessName {
    P2pool,
    Xmrig,
    Xvb,
}

impl std::fmt::Display for ProcessState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:#?}", self)
    }
}
impl std::fmt::Display for ProcessSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:#?}", self)
    }
}
impl std::fmt::Display for ProcessName {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            ProcessName::P2pool => write!(f, "P2Pool"),
            ProcessName::Xmrig => write!(f, "XMRig"),
            ProcessName::Xvb => write!(f, "XvB"),
        }
    }
}

//---------------------------------------------------------------------------------------------------- [Helper]
impl Helper {
    //---------------------------------------------------------------------------------------------------- General Functions
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instant: std::time::Instant,
        pub_sys: Arc<Mutex<Sys>>,
        p2pool: Arc<Mutex<Process>>,
        xmrig: Arc<Mutex<Process>>,
        xvb: Arc<Mutex<Process>>,
        gui_api_p2pool: Arc<Mutex<PubP2poolApi>>,
        gui_api_xmrig: Arc<Mutex<PubXmrigApi>>,
        gui_api_xvb: Arc<Mutex<PubXvbApi>>,
        img_p2pool: Arc<Mutex<ImgP2pool>>,
        img_xmrig: Arc<Mutex<ImgXmrig>>,
        gupax_p2pool_api: Arc<Mutex<GupaxP2poolApi>>,
    ) -> Self {
        Self {
            instant,
            pub_sys,
            uptime: HumanTime::into_human(instant.elapsed()),
            pub_api_p2pool: arc_mut!(PubP2poolApi::new()),
            pub_api_xmrig: arc_mut!(PubXmrigApi::new()),
            pub_api_xvb: arc_mut!(PubXvbApi::new()),
            // These are created when initializing [App], since it needs a handle to it as well
            p2pool,
            xmrig,
            xvb,
            gui_api_p2pool,
            gui_api_xmrig,
            gui_api_xvb,
            img_p2pool,
            img_xmrig,
            gupax_p2pool_api,
        }
    }

    // Reset output if larger than max bytes.
    // This will also append a message showing it was reset.
    fn check_reset_gui_output(output: &mut String, name: ProcessName) {
        let len = output.len();
        if len > GUI_OUTPUT_LEEWAY {
            info!(
                "{} Watchdog | Output is nearing {} bytes, resetting!",
                name, MAX_GUI_OUTPUT_BYTES
            );
            let text = format!("{}\n{} GUI log is exceeding the maximum: {} bytes!\nI've reset the logs for you!\n{}\n\n\n\n", HORI_CONSOLE, name, MAX_GUI_OUTPUT_BYTES, HORI_CONSOLE);
            output.clear();
            output.push_str(&text);
            debug!("{} Watchdog | Resetting GUI output ... OK", name);
        } else {
            debug!(
                "{} Watchdog | GUI output reset not needed! Current byte length ... {}",
                name, len
            );
        }
    }

    // Read P2Pool/XMRig's API file to a [String].
    fn path_to_string(
        path: &Path,
        name: ProcessName,
    ) -> std::result::Result<String, std::io::Error> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(s),
            Err(e) => {
                warn!("{} API | [{}] read error: {}", name, path.display(), e);
                Err(e)
            }
        }
    }
    //---------------------------------------------------------------------------------------------------- The "helper"
    #[inline(always)] // called once
    fn update_pub_sys_from_sysinfo(
        sysinfo: &sysinfo::System,
        pub_sys: &mut Sys,
        pid: &sysinfo::Pid,
        helper: &Helper,
        max_threads: usize,
    ) {
        let gupax_uptime = helper.uptime.to_string();
        let cpu = &sysinfo.cpus()[0];
        let gupax_cpu_usage = format!(
            "{:.2}%",
            sysinfo.process(*pid).unwrap().cpu_usage() / (max_threads as f32)
        );
        let gupax_memory_used_mb =
            HumanNumber::from_u64(sysinfo.process(*pid).unwrap().memory() / 1_000_000);
        let gupax_memory_used_mb = format!("{} megabytes", gupax_memory_used_mb);
        let system_cpu_model = format!("{} ({}MHz)", cpu.brand(), cpu.frequency());
        let system_memory = {
            let used = (sysinfo.used_memory() as f64) / 1_000_000_000.0;
            let total = (sysinfo.total_memory() as f64) / 1_000_000_000.0;
            format!("{:.3} GB / {:.3} GB", used, total)
        };
        let system_cpu_usage = {
            let mut total: f32 = 0.0;
            for cpu in sysinfo.cpus() {
                total += cpu.cpu_usage();
            }
            format!("{:.2}%", total / (max_threads as f32))
        };
        *pub_sys = Sys {
            gupax_uptime,
            gupax_cpu_usage,
            gupax_memory_used_mb,
            system_cpu_usage,
            system_memory,
            system_cpu_model,
        };
    }

    #[cold]
    #[inline(never)]
    // The "helper" thread. Syncs data between threads here and the GUI.
    #[allow(clippy::await_holding_lock)]
    pub fn spawn_helper(
        helper: &Arc<Mutex<Self>>,
        mut sysinfo: sysinfo::System,
        pid: sysinfo::Pid,
        max_threads: usize,
    ) {
        // The ordering of these locks is _very_ important. They MUST be in sync with how the main GUI thread locks stuff
        // or a deadlock will occur given enough time. They will eventually both want to lock the [Arc<Mutex>] the other
        // thread is already locking. Yes, I figured this out the hard way, hence the vast amount of debug!() messages.
        // Example of different order (BAD!):
        //
        // GUI Main       -> locks [p2pool] first
        // Helper         -> locks [gui_api_p2pool] first
        // GUI Status Tab -> tries to lock [gui_api_p2pool] -> CAN'T
        // Helper         -> tries to lock [p2pool] -> CAN'T
        //
        // These two threads are now in a deadlock because both
        // are trying to access locks the other one already has.
        //
        // The locking order here must be in the same chronological
        // order as the main GUI thread (top to bottom).

        let helper = Arc::clone(helper);
        let lock = lock!(helper);
        let p2pool = Arc::clone(&lock.p2pool);
        let xmrig = Arc::clone(&lock.xmrig);
        let xvb = Arc::clone(&lock.xvb);
        let pub_sys = Arc::clone(&lock.pub_sys);
        let gui_api_p2pool = Arc::clone(&lock.gui_api_p2pool);
        let gui_api_xmrig = Arc::clone(&lock.gui_api_xmrig);
        let gui_api_xvb = Arc::clone(&lock.gui_api_xvb);
        let pub_api_p2pool = Arc::clone(&lock.pub_api_p2pool);
        let pub_api_xmrig = Arc::clone(&lock.pub_api_xmrig);
        let pub_api_xvb = Arc::clone(&lock.pub_api_xvb);
        drop(lock);

        let sysinfo_cpu = sysinfo::CpuRefreshKind::everything();
        let sysinfo_processes = sysinfo::ProcessRefreshKind::new().with_cpu();

        thread::spawn(move || {
            info!("Helper | Hello from helper thread! Entering loop where I will spend the rest of my days...");
            // Begin loop
            loop {
                // 1. Loop init timestamp
                let start = Instant::now();
                debug!("Helper | ----------- Start of loop -----------");

                // Ignore the invasive [debug!()] messages on the right side of the code.
                // The reason why they are there are so that it's extremely easy to track
                // down the culprit of an [Arc<Mutex>] deadlock. I know, they're ugly.

                // 2. Lock... EVERYTHING!
                let mut lock = lock!(helper);
                debug!("Helper | Locking (1/11) ... [helper]");
                let p2pool = lock!(p2pool);
                debug!("Helper | Locking (2/11) ... [p2pool]");
                let xmrig = lock!(xmrig);
                debug!("Helper | Locking (3/11) ... [xmrig]");
                let xvb = lock!(xvb);
                debug!("Helper | Locking (4/11) ... [xvb]");
                let mut lock_pub_sys = lock!(pub_sys);
                debug!("Helper | Locking (5/11) ... [pub_sys]");
                let mut gui_api_p2pool = lock!(gui_api_p2pool);
                debug!("Helper | Locking (6/11) ... [gui_api_p2pool]");
                let mut gui_api_xmrig = lock!(gui_api_xmrig);
                debug!("Helper | Locking (7/11) ... [gui_api_xmrig]");
                let mut gui_api_xvb = lock!(gui_api_xvb);
                debug!("Helper | Locking (8/11) ... [gui_api_xvb]");
                let mut pub_api_p2pool = lock!(pub_api_p2pool);
                debug!("Helper | Locking (9/11) ... [pub_api_p2pool]");
                let mut pub_api_xmrig = lock!(pub_api_xmrig);
                debug!("Helper | Locking (10/11) ... [pub_api_xmrig]");
                let mut pub_api_xvb = lock!(pub_api_xvb);
                debug!("Helper | Locking (11/11) ... [pub_api_xvb]");
                // Calculate Gupax's uptime always.
                lock.uptime = HumanTime::into_human(lock.instant.elapsed());
                // If [P2Pool] is alive...
                if p2pool.is_alive() {
                    debug!("Helper | P2Pool is alive! Running [combine_gui_pub_api()]");
                    PubP2poolApi::combine_gui_pub_api(&mut gui_api_p2pool, &mut pub_api_p2pool);
                } else {
                    debug!("Helper | P2Pool is dead! Skipping...");
                }
                // If [XMRig] is alive...
                if xmrig.is_alive() {
                    debug!("Helper | XMRig is alive! Running [combine_gui_pub_api()]");
                    PubXmrigApi::combine_gui_pub_api(&mut gui_api_xmrig, &mut pub_api_xmrig);
                } else {
                    debug!("Helper | XMRig is dead! Skipping...");
                }
                // If [XvB] is alive...
                if xvb.is_alive() {
                    debug!("Helper | XvB is alive! Running [combine_gui_pub_api()]");
                    PubXvbApi::combine_gui_pub_api(&mut gui_api_xvb, &mut pub_api_xvb);
                } else {
                    debug!("Helper | XvB is dead! Skipping...");
                }

                // 2. Selectively refresh [sysinfo] for only what we need (better performance).
                sysinfo.refresh_cpu_specifics(sysinfo_cpu);
                debug!("Helper | Sysinfo refresh (1/3) ... [cpu]");
                sysinfo.refresh_processes_specifics(sysinfo_processes);
                debug!("Helper | Sysinfo refresh (2/3) ... [processes]");
                sysinfo.refresh_memory();
                debug!("Helper | Sysinfo refresh (3/3) ... [memory]");
                debug!("Helper | Sysinfo OK, running [update_pub_sys_from_sysinfo()]");
                Self::update_pub_sys_from_sysinfo(
                    &sysinfo,
                    &mut lock_pub_sys,
                    &pid,
                    &lock,
                    max_threads,
                );

                // 3. Drop... (almost) EVERYTHING... IN REVERSE!
                drop(lock_pub_sys);
                debug!("Helper | Unlocking (1/11) ... [pub_sys]");
                drop(xvb);
                debug!("Helper | Unlocking (2/11) ... [xvb]");
                drop(xmrig);
                debug!("Helper | Unlocking (3/11) ... [xmrig]");
                drop(p2pool);
                debug!("Helper | Unlocking (4/11) ... [p2pool]");
                drop(pub_api_xvb);
                debug!("Helper | Unlocking (5/11) ... [pub_api_xvb]");
                drop(pub_api_xmrig);
                debug!("Helper | Unlocking (6/11) ... [pub_api_xmrig]");
                drop(pub_api_p2pool);
                debug!("Helper | Unlocking (7/11) ... [pub_api_p2pool]");
                drop(gui_api_xvb);
                debug!("Helper | Unlocking (8/11) ... [gui_api_xvb]");
                drop(gui_api_xmrig);
                debug!("Helper | Unlocking (9/11) ... [gui_api_xmrig]");
                drop(gui_api_p2pool);
                debug!("Helper | Unlocking (10/11) ... [gui_api_p2pool]");
                drop(lock);
                debug!("Helper | Unlocking (11/11) ... [helper]");

                // 4. Calculate if we should sleep or not.
                // If we should sleep, how long?
                let elapsed = start.elapsed().as_millis();
                if elapsed < 1000 {
                    // Casting from u128 to u64 should be safe here, because [elapsed]
                    // is less than 1000, meaning it can fit into a u64 easy.
                    let sleep = (1000 - elapsed) as u64;
                    debug!("Helper | END OF LOOP - Sleeping for [{}]ms...", sleep);
                    sleep!(sleep);
                } else {
                    debug!("Helper | END OF LOOP - Not sleeping!");
                }

                // 5. End loop
            }
        });
    }
}
