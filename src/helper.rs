// Gupax - GUI Uniting P2Pool And XMRig
//
// Copyright (c) 2022 hinto-janaiyo
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
//     if p2pool.lock().unwrap().signal == ProcessSignal::Stop {
//         stop_p2pool(),
//     }
//
// This also includes all things related to handling the child processes (P2Pool/XMRig):
// piping their stdout/stderr/stdin, accessing their APIs (HTTP + disk files), etc.

//---------------------------------------------------------------------------------------------------- Import
use std::{
	sync::{Arc,Mutex},
	path::PathBuf,
	process::Stdio,
	fmt::Write,
	time::*,
	thread,
};
use crate::{
	constants::*,
	SudoState,
};
use sysinfo::SystemExt;
use serde::{Serialize,Deserialize};
use sysinfo::{CpuExt,ProcessExt};
use log::*;

//---------------------------------------------------------------------------------------------------- Constants
// The locale numbers are formatting in is English, which looks like: [1,000]
const LOCALE: num_format::Locale = num_format::Locale::en;
// The max amount of bytes of process output we are willing to
// hold in memory before it's too much and we need to reset.
const MAX_GUI_OUTPUT_BYTES: usize = 500_000;
// Just a little leeway so a reset will go off before the [String] allocates more memory.
const GUI_OUTPUT_LEEWAY: usize = MAX_GUI_OUTPUT_BYTES - 1000;


//---------------------------------------------------------------------------------------------------- [Helper] Struct
// A meta struct holding all the data that gets processed in this thread
pub struct Helper {
	pub instant: Instant,                         // Gupax start as an [Instant]
	pub uptime: HumanTime,                        // Gupax uptime formatting for humans
	pub pub_sys: Arc<Mutex<Sys>>,                 // The public API for [sysinfo] that the [Status] tab reads from
	pub p2pool: Arc<Mutex<Process>>,              // P2Pool process state
	pub xmrig: Arc<Mutex<Process>>,               // XMRig process state
	pub gui_api_p2pool: Arc<Mutex<PubP2poolApi>>, // P2Pool API state (for GUI thread)
	pub gui_api_xmrig: Arc<Mutex<PubXmrigApi>>,   // XMRig API state (for GUI thread)
	pub img_p2pool: Arc<Mutex<ImgP2pool>>,        // A static "image" of the data P2Pool started with
	pub img_xmrig: Arc<Mutex<ImgXmrig>>,          // A static "image" of the data XMRig started with
	pub_api_p2pool: Arc<Mutex<PubP2poolApi>>,     // P2Pool API state (for Helper/P2Pool thread)
	pub_api_xmrig: Arc<Mutex<PubXmrigApi>>,       // XMRig API state (for Helper/XMRig thread)
	priv_api_p2pool: Arc<Mutex<PrivP2poolApi>>,   // For "watchdog" thread
	priv_api_xmrig: Arc<Mutex<PrivXmrigApi>>,     // For "watchdog" thread
}

// The communication between the data here and the GUI thread goes as follows:
// [GUI] <---> [Helper] <---> [Watchdog] <---> [Private Data only available here]
//
// Both [GUI] and [Helper] own their separate [Pub*Api] structs.
// Since P2Pool & XMRig will be updating their information out of sync,
// it's the helpers job to lock everything, and move the watchdog [Pub*Api]s
// on a 1-second interval into the [GUI]'s [Pub*Api] struct, atomically.

//----------------------------------------------------------------------------------------------------
#[derive(Debug,Clone)]
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
	child: Option<Arc<Mutex<Box<dyn portable_pty::Child + Send + std::marker::Sync>>>>, // STDOUT/STDERR is combined automatically thanks to this PTY, nice
	stdin: Option<Box<dyn portable_pty::MasterPty + Send>>, // A handle to the process's MasterPTY/STDIN

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
			stdin: Option::None,
			child: Option::None,
			output_parse: Arc::new(Mutex::new(String::with_capacity(500))),
			output_pub: Arc::new(Mutex::new(String::with_capacity(500))),
			input: vec![String::new()],
		}
	}

	// Borrow a [&str], return an owned split collection
	pub fn parse_args(args: &str) -> Vec<String> {
		args.split_whitespace().map(|s| s.to_owned()).collect()
	}

	// Convenience functions
	pub fn is_alive(&self) -> bool {
		self.state == ProcessState::Alive || self.state == ProcessState::Middle
	}

	pub fn is_waiting(&self) -> bool {
		self.state == ProcessState::Middle || self.state == ProcessState::Waiting
	}
}

//---------------------------------------------------------------------------------------------------- [Process*] Enum
#[derive(Copy,Clone,Eq,PartialEq,Debug)]
pub enum ProcessState {
	Alive,  // Process is online, GREEN!
	Dead,   // Process is dead, BLACK!
	Failed, // Process is dead AND exited with a bad code, RED!
	Middle, // Process is in the middle of something ([re]starting/stopping), YELLOW!
	Waiting, // Process was successfully killed by a restart, and is ready to be started again, YELLOW!
}

#[derive(Copy,Clone,Eq,PartialEq,Debug)]
pub enum ProcessSignal {
	None,
	Start,
	Stop,
	Restart,
}

#[derive(Copy,Clone,Eq,PartialEq,Debug)]
pub enum ProcessName {
	P2pool,
	Xmrig,
}

impl std::fmt::Display for ProcessState  { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{:#?}", self) } }
impl std::fmt::Display for ProcessSignal { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{:#?}", self) } }
impl std::fmt::Display for ProcessName   {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match *self {
			ProcessName::P2pool => write!(f, "P2Pool"),
			ProcessName::Xmrig => write!(f, "XMRig"),
		}
	}
}

//---------------------------------------------------------------------------------------------------- [Helper]
impl Helper {
	//---------------------------------------------------------------------------------------------------- General Functions
	pub fn new(instant: std::time::Instant, pub_sys: Arc<Mutex<Sys>>, p2pool: Arc<Mutex<Process>>, xmrig: Arc<Mutex<Process>>, gui_api_p2pool: Arc<Mutex<PubP2poolApi>>, gui_api_xmrig: Arc<Mutex<PubXmrigApi>>, img_p2pool: Arc<Mutex<ImgP2pool>>, img_xmrig: Arc<Mutex<ImgXmrig>>) -> Self {
		Self {
			instant,
			pub_sys,
			uptime: HumanTime::into_human(instant.elapsed()),
			priv_api_p2pool: Arc::new(Mutex::new(PrivP2poolApi::new())),
			priv_api_xmrig: Arc::new(Mutex::new(PrivXmrigApi::new())),
			pub_api_p2pool: Arc::new(Mutex::new(PubP2poolApi::new())),
			pub_api_xmrig: Arc::new(Mutex::new(PubXmrigApi::new())),
			// These are created when initializing [App], since it needs a handle to it as well
			p2pool,
			xmrig,
			gui_api_p2pool,
			gui_api_xmrig,
			img_p2pool,
			img_xmrig,
		}
	}

	// Reads a PTY which combines STDOUT/STDERR for me, yay
	fn read_pty(output_parse: Arc<Mutex<String>>, output_pub: Arc<Mutex<String>>, reader: Box<dyn std::io::Read + Send>, name: ProcessName) {
		use std::io::BufRead;
		let mut stdout = std::io::BufReader::new(reader).lines();
		// We don't need to write twice for XMRig, since we dont parse it... yet.
		if name == ProcessName::Xmrig {
			while let Some(Ok(line)) = stdout.next() {
//				println!("{}", line); // For debugging.
//				if let Err(e) = writeln!(output_parse.lock().unwrap(), "{}", line) { error!("PTY | Output error: {}", e); }
				if let Err(e) = writeln!(output_pub.lock().unwrap(), "{}", line) { error!("PTY | Output error: {}", e); }
			}
		} else {
			while let Some(Ok(line)) = stdout.next() {
//				println!("{}", line); // For debugging.
				if let Err(e) = writeln!(output_parse.lock().unwrap(), "{}", line) { error!("PTY | Output error: {}", e); }
				if let Err(e) = writeln!(output_pub.lock().unwrap(), "{}", line) { error!("PTY | Output error: {}", e); }
			}
		}
	}

	// Reset output if larger than max bytes.
	// This will also append a message showing it was reset.
	fn check_reset_gui_output(output: &mut String, name: ProcessName) {
		let len = output.len();
		if len > GUI_OUTPUT_LEEWAY {
			info!("{} Watchdog | Output is nearing {} bytes, resetting!", name, MAX_GUI_OUTPUT_BYTES);
			let text = format!("{}\n{} GUI log is exceeding the maximum: {} bytes!\nI've reset the logs for you!\n{}\n\n\n\n", HORI_CONSOLE, name, MAX_GUI_OUTPUT_BYTES, HORI_CONSOLE);
			output.clear();
			output.push_str(&text);
			debug!("{} Watchdog | Resetting GUI output ... OK", name);
		} else {
			debug!("{} Watchdog | GUI output reset not needed! Current byte length ... {}", name, len);
		}
	}

	//---------------------------------------------------------------------------------------------------- P2Pool specific
	// Just sets some signals for the watchdog thread to pick up on.
	pub fn stop_p2pool(helper: &Arc<Mutex<Self>>) {
		info!("P2Pool | Attempting to stop...");
		helper.lock().unwrap().p2pool.lock().unwrap().signal = ProcessSignal::Stop;
		helper.lock().unwrap().p2pool.lock().unwrap().state = ProcessState::Middle;
	}

	// The "restart frontend" to a "frontend" function.
	// Basically calls to kill the current p2pool, waits a little, then starts the below function in a a new thread, then exit.
	pub fn restart_p2pool(helper: &Arc<Mutex<Self>>, state: &crate::disk::P2pool, path: &std::path::PathBuf) {
		info!("P2Pool | Attempting to restart...");
		helper.lock().unwrap().p2pool.lock().unwrap().signal = ProcessSignal::Restart;
		helper.lock().unwrap().p2pool.lock().unwrap().state = ProcessState::Middle;

		let helper = Arc::clone(helper);
		let state = state.clone();
		let path = path.clone();
		// This thread lives to wait, start p2pool then die.
		thread::spawn(move || {
			while helper.lock().unwrap().p2pool.lock().unwrap().is_alive() {
				warn!("P2Pool | Want to restart but process is still alive, waiting...");
				thread::sleep(SECOND);
			}
			// Ok, process is not alive, start the new one!
			info!("P2Pool | Old process seems dead, starting new one!");
			Self::start_p2pool(&helper, &state, &path);
		});
		info!("P2Pool | Restart ... OK");
	}

	// The "frontend" function that parses the arguments, and spawns either the [Simple] or [Advanced] P2Pool watchdog thread.
	pub fn start_p2pool(helper: &Arc<Mutex<Self>>, state: &crate::disk::P2pool, path: &std::path::PathBuf) {
		helper.lock().unwrap().p2pool.lock().unwrap().state = ProcessState::Middle;

		let (args, api_path) = Self::build_p2pool_args_and_mutate_img(helper, state, path);

		// Print arguments & user settings to console
		crate::disk::print_dash(&format!("P2Pool | Launch arguments: {:#?} | API Path: {:#?}", args, api_path));

		// Spawn watchdog thread
		let process = Arc::clone(&helper.lock().unwrap().p2pool);
		let gui_api = Arc::clone(&helper.lock().unwrap().gui_api_p2pool);
		let pub_api = Arc::clone(&helper.lock().unwrap().pub_api_p2pool);
		let priv_api = Arc::clone(&helper.lock().unwrap().priv_api_p2pool);
		let path = path.clone();
		thread::spawn(move || {
			Self::spawn_p2pool_watchdog(process, gui_api, pub_api, priv_api, args, path, api_path);
		});
	}

	// Takes in some [State/P2pool] and parses it to build the actual command arguments.
	// Returns the [Vec] of actual arguments, and mutates the [ImgP2pool] for the main GUI thread
	// It returns a value... and mutates a deeply nested passed argument... this is some pretty bad code...
	pub fn build_p2pool_args_and_mutate_img(helper: &Arc<Mutex<Self>>, state: &crate::disk::P2pool, path: &std::path::PathBuf) -> (Vec<String>, PathBuf) {
		let mut args = Vec::with_capacity(500);
		let path = path.clone();
		let mut api_path = path;
		api_path.pop();

		// [Simple]
		if state.simple {
			// Build the p2pool argument
			let (ip, rpc, zmq) = crate::node::enum_to_ip_rpc_zmq_tuple(state.node);         // Get: (IP, RPC, ZMQ)
			args.push("--wallet".to_string());   args.push(state.address.clone());          // Wallet address
			args.push("--host".to_string());     args.push(ip.to_string());                 // IP Address
			args.push("--rpc-port".to_string()); args.push(rpc.to_string());                // RPC Port
			args.push("--zmq-port".to_string()); args.push(zmq.to_string());                // ZMQ Port
			args.push("--data-api".to_string()); args.push(api_path.display().to_string()); // API Path
			args.push("--local-api".to_string()); // Enable API
			args.push("--no-color".to_string());  // Remove color escape sequences, Gupax terminal can't parse it :(
			args.push("--mini".to_string());      // P2Pool Mini
			*helper.lock().unwrap().img_p2pool.lock().unwrap() = ImgP2pool {
				mini: true,
				address: state.address.clone(),
				host: ip.to_string(),
				rpc: rpc.to_string(),
				zmq: zmq.to_string(),
				log_level: "3".to_string(),
				out_peers: "10".to_string(),
				in_peers: "10".to_string(),
			};
			api_path.push(P2POOL_API_PATH);

		// [Advanced]
		} else {
			// Overriding command arguments
			if !state.arguments.is_empty() {
				// This parses the input and attemps to fill out
				// the [ImgP2pool]... This is pretty bad code...
				let mut last = "";
				let lock = helper.lock().unwrap();
				let mut p2pool_image = lock.img_p2pool.lock().unwrap();
				for arg in state.arguments.split_whitespace() {
					match last {
						"--mini"      => p2pool_image.mini = true,
						"--wallet"    => p2pool_image.address = arg.to_string(),
						"--host"      => p2pool_image.host = arg.to_string(),
						"--rpc-port"  => p2pool_image.rpc = arg.to_string(),
						"--zmq-port"  => p2pool_image.zmq = arg.to_string(),
						"--loglevel"  => p2pool_image.log_level = arg.to_string(),
						"--out-peers" => p2pool_image.out_peers = arg.to_string(),
						"--in-peers"  => p2pool_image.in_peers = arg.to_string(),
						"--data-api"  => api_path = PathBuf::from(arg),
						_ => (),
					}
					args.push(arg.to_string());
					last = arg;
				}
			// Else, build the argument
			} else {
				args.push("--wallet".to_string());    args.push(state.address.clone());          // Wallet
				args.push("--host".to_string());      args.push(state.selected_ip.to_string());  // IP
				args.push("--rpc-port".to_string());  args.push(state.selected_rpc.to_string()); // RPC
				args.push("--zmq-port".to_string());  args.push(state.selected_zmq.to_string()); // ZMQ
				args.push("--loglevel".to_string());  args.push(state.log_level.to_string());    // Log Level
				args.push("--out-peers".to_string()); args.push(state.out_peers.to_string());    // Out Peers
				args.push("--in-peers".to_string());  args.push(state.in_peers.to_string());     // In Peers
				args.push("--data-api".to_string());  args.push(api_path.display().to_string()); // API Path
				args.push("--local-api".to_string());               // Enable API
				args.push("--no-color".to_string());                // Remove color escape sequences
				if state.mini { args.push("--mini".to_string()); }; // Mini
				*helper.lock().unwrap().img_p2pool.lock().unwrap() = ImgP2pool {
					mini: state.mini,
					address: state.address.clone(),
					host: state.selected_ip.to_string(),
					rpc: state.selected_rpc.to_string(),
					zmq: state.selected_zmq.to_string(),
					log_level: state.log_level.to_string(),
					out_peers: state.out_peers.to_string(),
					in_peers: state.in_peers.to_string(),
				};
				api_path.push(P2POOL_API_PATH);
			}
		}
		(args, api_path)
	}

	// The P2Pool watchdog. Spawns 1 OS thread for reading a PTY (STDOUT+STDERR), and combines the [Child] with a PTY so STDIN actually works.
	fn spawn_p2pool_watchdog(process: Arc<Mutex<Process>>, gui_api: Arc<Mutex<PubP2poolApi>>, pub_api: Arc<Mutex<PubP2poolApi>>, _priv_api: Arc<Mutex<PrivP2poolApi>>, args: Vec<String>, path: std::path::PathBuf, api_path: std::path::PathBuf) {
		// 1a. Create PTY
		debug!("P2Pool | Creating PTY...");
		let pty = portable_pty::native_pty_system();
		let pair = pty.openpty(portable_pty::PtySize {
			rows: 100,
			cols: 1000,
			pixel_width: 0,
			pixel_height: 0,
		}).unwrap();
		// 1b. Create command
		debug!("P2Pool | Creating command...");
		let mut cmd = portable_pty::CommandBuilder::new(path.as_path());
		cmd.args(args);
		cmd.cwd(path.as_path().parent().unwrap());
		// 1c. Create child
		debug!("P2Pool | Creating child...");
		let child_pty = Arc::new(Mutex::new(pair.slave.spawn_command(cmd).unwrap()));

        // 2. Set process state
		debug!("P2Pool | Setting process state...");
        let mut lock = process.lock().unwrap();
        lock.state = ProcessState::Alive;
        lock.signal = ProcessSignal::None;
        lock.start = Instant::now();
		lock.child = Some(Arc::clone(&child_pty));
		let reader = pair.master.try_clone_reader().unwrap(); // Get STDOUT/STDERR before moving the PTY
		lock.stdin = Some(pair.master);
		drop(lock);

		// 3. Spawn PTY read thread
		debug!("P2Pool | Spawning PTY read thread...");
		let output_parse = Arc::clone(&process.lock().unwrap().output_parse);
		let output_pub = Arc::clone(&process.lock().unwrap().output_pub);
		thread::spawn(move || {
			Self::read_pty(output_parse, output_pub, reader, ProcessName::P2pool);
		});
		let output_parse = Arc::clone(&process.lock().unwrap().output_parse);
		let output_pub = Arc::clone(&process.lock().unwrap().output_pub);

		debug!("P2Pool | Cleaning old API files...");
		// Attempt to remove stale API file
		match std::fs::remove_file(&api_path) {
			Ok(_) => info!("P2Pool | Attempting to remove stale API file ... OK"),
			Err(e) => warn!("P2Pool | Attempting to remove stale API file ... FAIL ... {}", e),
		}
		// Attempt to create a default empty one.
		use std::io::Write;
		if std::fs::File::create(&api_path).is_ok() {
			let text = r#"{"hashrate_15m":0,"hashrate_1h":0,"hashrate_24h":0,"shares_found":0,"average_effort":0.0,"current_effort":0.0,"connections":0}"#;
			match std::fs::write(&api_path, text) {
				Ok(_) => info!("P2Pool | Creating default empty API file ... OK"),
				Err(e) => warn!("P2Pool | Creating default empty API file ... FAIL ... {}", e),
			}
		}
		let regex = P2poolRegex::new();
		let start = process.lock().unwrap().start;

		// 4. Loop as watchdog
		info!("P2Pool | Entering watchdog mode... woof!");
		loop {
			// Set timer
			let now = Instant::now();
			debug!("P2Pool Watchdog | ----------- Start of loop -----------");

			// Check if the process is secretly died without us knowing :)
			if let Ok(Some(code)) = child_pty.lock().unwrap().try_wait() {
				debug!("P2Pool Watchdog | Process secretly died! Getting exit status");
				let exit_status = match code.success() {
					true  => { process.lock().unwrap().state = ProcessState::Dead; "Successful" },
					false => { process.lock().unwrap().state = ProcessState::Failed; "Failed" },
				};
				let uptime = HumanTime::into_human(start.elapsed());
				info!("P2Pool Watchdog | Stopped ... Uptime was: [{}], Exit status: [{}]", uptime, exit_status);
				// This is written directly into the GUI, because sometimes the 900ms event loop can't catch it.
				if let Err(e) = writeln!(
					gui_api.lock().unwrap().output,
					"{}\nP2Pool stopped | Uptime: [{}] | Exit status: [{}]\n{}\n\n\n\n",
					HORI_CONSOLE,
					uptime,
					exit_status,
					HORI_CONSOLE
				) {
					error!("P2Pool Watchdog | GUI Uptime/Exit status write failed: {}", e);
				}
				process.lock().unwrap().signal = ProcessSignal::None;
				debug!("P2Pool Watchdog | Secret dead process reap OK, breaking");
				break
			}

			// Check SIGNAL
			if process.lock().unwrap().signal == ProcessSignal::Stop {
				debug!("P2Pool Watchdog | Stop SIGNAL caught");
				// This actually sends a SIGHUP to p2pool (closes the PTY, hangs up on p2pool)
				if let Err(e) = child_pty.lock().unwrap().kill() { error!("P2Pool Watchdog | Kill error: {}", e); }
				// Wait to get the exit status
				let exit_status = match child_pty.lock().unwrap().wait() {
					Ok(e) => {
						if e.success() {
							process.lock().unwrap().state = ProcessState::Dead; "Successful"
						} else {
							process.lock().unwrap().state = ProcessState::Failed; "Failed"
						}
					},
					_ => { process.lock().unwrap().state = ProcessState::Failed; "Unknown Error" },
				};
				let uptime = HumanTime::into_human(start.elapsed());
				info!("P2Pool Watchdog | Stopped ... Uptime was: [{}], Exit status: [{}]", uptime, exit_status);
				// This is written directly into the GUI API, because sometimes the 900ms event loop can't catch it.
				if let Err(e) = writeln!(
					gui_api.lock().unwrap().output,
					"{}\nP2Pool stopped | Uptime: [{}] | Exit status: [{}]\n{}\n\n\n\n",
					HORI_CONSOLE,
					uptime,
					exit_status,
					HORI_CONSOLE
				) {
					error!("P2Pool Watchdog | GUI Uptime/Exit status write failed: {}", e);
				}
				process.lock().unwrap().signal = ProcessSignal::None;
				debug!("P2Pool Watchdog | Stop SIGNAL done, breaking");
				break
			// Check RESTART
			} else if process.lock().unwrap().signal == ProcessSignal::Restart {
				debug!("P2Pool Watchdog | Restart SIGNAL caught");
				// This actually sends a SIGHUP to p2pool (closes the PTY, hangs up on p2pool)
				if let Err(e) = child_pty.lock().unwrap().kill() { error!("P2Pool Watchdog | Kill error: {}", e); }
				// Wait to get the exit status
				let exit_status = match child_pty.lock().unwrap().wait() {
					Ok(e) => if e.success() { "Successful" } else { "Failed" },
					_ => "Unknown Error",
				};
				let uptime = HumanTime::into_human(start.elapsed());
				info!("P2Pool Watchdog | Stopped ... Uptime was: [{}], Exit status: [{}]", uptime, exit_status);
				// This is written directly into the GUI API, because sometimes the 900ms event loop can't catch it.
				if let Err(e) = writeln!(
					gui_api.lock().unwrap().output,
					"{}\nP2Pool stopped | Uptime: [{}] | Exit status: [{}]\n{}\n\n\n\n",
					HORI_CONSOLE,
					uptime,
					exit_status,
					HORI_CONSOLE
				) {
					error!("P2Pool Watchdog | GUI Uptime/Exit status write failed: {}", e);
				}
				process.lock().unwrap().state = ProcessState::Waiting;
				debug!("P2Pool Watchdog | Restart SIGNAL done, breaking");
				break
			}

			// Check vector of user input
			let mut lock = process.lock().unwrap();
			if !lock.input.is_empty() {
				let input = std::mem::take(&mut lock.input);
				for line in input {
					debug!("P2Pool Watchdog | User input not empty, writing to STDIN: [{}]", line);
					if let Err(e) = writeln!(lock.stdin.as_mut().unwrap(), "{}", line) { error!("P2Pool Watchdog | STDIN error: {}", e); }
				}
			}
			drop(lock);


			// Check if logs need resetting
			debug!("P2Pool Watchdog | Attempting GUI log reset check");
			let mut lock = gui_api.lock().unwrap();
			Self::check_reset_gui_output(&mut lock.output, ProcessName::P2pool);
			drop(lock);

			// Always update from output
			debug!("P2Pool Watchdog | Starting [update_from_output()]");
			PubP2poolApi::update_from_output(&pub_api, &output_parse, &output_pub, start.elapsed(), &regex);

			// Read API file into string
			debug!("P2Pool Watchdog | Attempting API file read");
			if let Ok(string) = PrivP2poolApi::read_p2pool_api(&api_path) {
				// Deserialize
				if let Ok(s) = PrivP2poolApi::str_to_priv_p2pool_api(&string) {
					// Update the structs.
					PubP2poolApi::update_from_priv(&pub_api, s);
				}
			}

			// Sleep (only if 900ms hasn't passed)
			let elapsed = now.elapsed().as_millis();
			// Since logic goes off if less than 1000, casting should be safe
			if elapsed < 900 {
				let sleep = (900-elapsed) as u64;
				debug!("P2Pool Watchdog | END OF LOOP - Sleeping for [{}]ms...", sleep);
				std::thread::sleep(std::time::Duration::from_millis(sleep));
			} else {
				debug!("P2Pool Watchdog | END OF LOOP - Not sleeping!");
			}
		}

		// 5. If loop broke, we must be done here.
		info!("P2Pool Watchdog | Watchdog thread exiting... Goodbye!");
	}

	//---------------------------------------------------------------------------------------------------- XMRig specific, most functions are very similar to P2Pool's
	// If processes are started with [sudo] on macOS, they must also
	// be killed with [sudo] (even if I have a direct handle to it as the
	// parent process...!). This is only needed on macOS, not Linux.
	fn sudo_kill(pid: u32, sudo: &Arc<Mutex<SudoState>>) -> bool {
		// Spawn [sudo] to execute [kill] on the given [pid]
		let mut child = std::process::Command::new("sudo")
			.args(["--stdin", "kill", "-9", &pid.to_string()])
			.stdin(Stdio::piped())
			.spawn().unwrap();

		// Write the [sudo] password to STDIN.
		let mut stdin = child.stdin.take().unwrap();
		use std::io::Write;
		if let Err(e) = writeln!(stdin, "{}\n", sudo.lock().unwrap().pass) { error!("Sudo Kill | STDIN error: {}", e); }

		// Return exit code of [sudo/kill].
		child.wait().unwrap().success()
	}

	// Just sets some signals for the watchdog thread to pick up on.
	pub fn stop_xmrig(helper: &Arc<Mutex<Self>>) {
		info!("XMRig | Attempting to stop...");
		helper.lock().unwrap().xmrig.lock().unwrap().signal = ProcessSignal::Stop;
		helper.lock().unwrap().xmrig.lock().unwrap().state = ProcessState::Middle;
	}

	// The "restart frontend" to a "frontend" function.
	// Basically calls to kill the current xmrig, waits a little, then starts the below function in a a new thread, then exit.
	pub fn restart_xmrig(helper: &Arc<Mutex<Self>>, state: &crate::disk::Xmrig, path: &std::path::PathBuf, sudo: Arc<Mutex<SudoState>>) {
		info!("XMRig | Attempting to restart...");
		helper.lock().unwrap().xmrig.lock().unwrap().signal = ProcessSignal::Restart;
		helper.lock().unwrap().xmrig.lock().unwrap().state = ProcessState::Middle;

		let helper = Arc::clone(helper);
		let state = state.clone();
		let path = path.clone();
		// This thread lives to wait, start xmrig then die.
		thread::spawn(move || {
			while helper.lock().unwrap().xmrig.lock().unwrap().state != ProcessState::Waiting {
				warn!("XMRig | Want to restart but process is still alive, waiting...");
				thread::sleep(SECOND);
			}
			// Ok, process is not alive, start the new one!
			info!("XMRig | Old process seems dead, starting new one!");
			Self::start_xmrig(&helper, &state, &path, sudo);
		});
		info!("XMRig | Restart ... OK");
	}

	pub fn start_xmrig(helper: &Arc<Mutex<Self>>, state: &crate::disk::Xmrig, path: &std::path::PathBuf, sudo: Arc<Mutex<SudoState>>) {
		helper.lock().unwrap().xmrig.lock().unwrap().state = ProcessState::Middle;

		let (args, api_ip_port) = Self::build_xmrig_args_and_mutate_img(helper, state, path);

		// Print arguments & user settings to console
		crate::disk::print_dash(&format!("XMRig | Launch arguments: {:#?}", args));
		info!("XMRig | Using path: [{}]", path.display());

		// Spawn watchdog thread
		let process = Arc::clone(&helper.lock().unwrap().xmrig);
		let gui_api = Arc::clone(&helper.lock().unwrap().gui_api_xmrig);
		let pub_api = Arc::clone(&helper.lock().unwrap().pub_api_xmrig);
		let priv_api = Arc::clone(&helper.lock().unwrap().priv_api_xmrig);
		let path = path.clone();
		thread::spawn(move || {
			Self::spawn_xmrig_watchdog(process, gui_api, pub_api, priv_api, args, path, sudo, api_ip_port);
		});
	}

	// Takes in some [State/Xmrig] and parses it to build the actual command arguments.
	// Returns the [Vec] of actual arguments, and mutates the [ImgXmrig] for the main GUI thread
	// It returns a value... and mutates a deeply nested passed argument... this is some pretty bad code...
	pub fn build_xmrig_args_and_mutate_img(helper: &Arc<Mutex<Self>>, state: &crate::disk::Xmrig, path: &std::path::PathBuf) -> (Vec<String>, String) {
		let mut args = Vec::with_capacity(500);
		let mut api_ip = String::with_capacity(15);
		let mut api_port = String::with_capacity(5);
		let path = path.clone();
		// The actual binary we're executing is [sudo], technically
		// the XMRig path is just an argument to sudo, so add it.
		// Before that though, add the ["--prompt"] flag and set it
		// to emptyness so that it doesn't show up in the output.
		if cfg!(unix) {
			args.push(r#"--prompt="#.to_string());
			args.push("--".to_string());
			args.push(path.display().to_string());
		}

		// [Simple]
		if state.simple {
			// Build the xmrig argument
			let rig = if state.simple_rig.is_empty() { GUPAX_VERSION_UNDERSCORE.to_string() } else { state.simple_rig.clone() }; // Rig name
			args.push("--url".to_string()); args.push("127.0.0.1:3333".to_string());          // Local P2Pool (the default)
			args.push("--threads".to_string()); args.push(state.current_threads.to_string()); // Threads
			args.push("--user".to_string()); args.push(rig);                                  // Rig name
			args.push("--no-color".to_string());                                              // No color
			args.push("--http-host".to_string()); args.push("127.0.0.1".to_string());         // HTTP API IP
			args.push("--http-port".to_string()); args.push("18088".to_string());             // HTTP API Port
			if state.pause != 0 { args.push("--pause-on-active".to_string()); args.push(state.pause.to_string()); } // Pause on active
			*helper.lock().unwrap().img_xmrig.lock().unwrap() = ImgXmrig {
				threads: state.current_threads.to_string(),
				url: "127.0.0.1:3333 (Local P2Pool)".to_string(),
			};
			api_ip = "127.0.0.1".to_string();
			api_port = "18088".to_string();

		// [Advanced]
		} else {
			// Overriding command arguments
			if !state.arguments.is_empty() {
				// This parses the input and attemps to fill out
				// the [ImgXmrig]... This is pretty bad code...
				let mut last = "";
				let lock = helper.lock().unwrap();
				let mut xmrig_image = lock.img_xmrig.lock().unwrap();
				for arg in state.arguments.split_whitespace() {
					match last {
						"--threads"   => xmrig_image.threads = arg.to_string(),
						"--url"       => xmrig_image.url = arg.to_string(),
						"--http-host" => api_ip = arg.to_string(),
						"--http-port" => api_port = arg.to_string(),
						_ => (),
					}
					args.push(arg.to_string());
					last = arg;
				}
			// Else, build the argument
			} else {
				// XMRig doesn't understand [localhost]
				api_ip = if state.api_ip == "localhost" || state.api_ip.is_empty() { "127.0.0.1".to_string() } else { state.api_ip.to_string() };
				api_port = if state.api_port.is_empty() { "18088".to_string() } else { state.api_port.to_string() };
				let url = format!("{}:{}", state.selected_ip, state.selected_port); // Combine IP:Port into one string
				args.push("--user".to_string()); args.push(state.address.clone());                // Wallet
				args.push("--threads".to_string()); args.push(state.current_threads.to_string()); // Threads
				args.push("--rig-id".to_string()); args.push(state.selected_rig.to_string());     // Rig ID
				args.push("--url".to_string()); args.push(url.clone());                           // IP/Port
				args.push("--http-host".to_string()); args.push(api_ip.to_string());              // HTTP API IP
				args.push("--http-port".to_string()); args.push(api_port.to_string());            // HTTP API Port
				args.push("--no-color".to_string());                         // No color escape codes
				if state.tls { args.push("--tls".to_string()); }             // TLS
				if state.keepalive { args.push("--keepalive".to_string()); } // Keepalive
				if state.pause != 0 { args.push("--pause-on-active".to_string()); args.push(state.pause.to_string()); } // Pause on active
				*helper.lock().unwrap().img_xmrig.lock().unwrap() = ImgXmrig {
					url,
					threads: state.current_threads.to_string(),
				};
			}
		}
		(args, format!("{}:{}", api_ip, api_port))
	}

	// We actually spawn [sudo] on Unix, with XMRig being the argument.
	#[cfg(target_family = "unix")]
	fn create_xmrig_cmd_unix(args: Vec<String>, path: PathBuf) -> portable_pty::CommandBuilder {
		let mut cmd = portable_pty::cmdbuilder::CommandBuilder::new("sudo");
		cmd.args(args);
		cmd.cwd(path.as_path().parent().unwrap());
		cmd
	}

	// Gupax should be admin on Windows, so just spawn XMRig normally.
	#[cfg(target_os = "windows")]
	fn create_xmrig_cmd_windows(args: Vec<String>, path: PathBuf) -> portable_pty::CommandBuilder {
		let mut cmd = portable_pty::cmdbuilder::CommandBuilder::new(path.clone());
		cmd.args(args);
		cmd.cwd(path.as_path().parent().unwrap());
		cmd
	}

	// The XMRig watchdog. Spawns 1 OS thread for reading a PTY (STDOUT+STDERR), and combines the [Child] with a PTY so STDIN actually works.
	// This isn't actually async, a tokio runtime is unfortunately needed because [Hyper] is an async library (HTTP API calls)
	#[tokio::main]
	async fn spawn_xmrig_watchdog(process: Arc<Mutex<Process>>, gui_api: Arc<Mutex<PubXmrigApi>>, pub_api: Arc<Mutex<PubXmrigApi>>, _priv_api: Arc<Mutex<PrivXmrigApi>>, args: Vec<String>, path: std::path::PathBuf, sudo: Arc<Mutex<SudoState>>, api_ip_port: String) {
		// 1a. Create PTY
		debug!("XMRig | Creating PTY...");
		let pty = portable_pty::native_pty_system();
		let mut pair = pty.openpty(portable_pty::PtySize {
			rows: 100,
			cols: 1000,
			pixel_width: 0,
			pixel_height: 0,
		}).unwrap();
		// 1b. Create command
		debug!("XMRig | Creating command...");
		#[cfg(target_os = "windows")]
		let cmd = Self::create_xmrig_cmd_windows(args, path);
		#[cfg(target_family = "unix")]
		let cmd = Self::create_xmrig_cmd_unix(args, path);
		// 1c. Create child
		debug!("XMRig | Creating child...");
		let child_pty = Arc::new(Mutex::new(pair.slave.spawn_command(cmd).unwrap()));

		// 2. Input [sudo] pass, wipe, then drop.
		if cfg!(unix) {
			debug!("XMRig | Inputting [sudo] and wiping...");
			// 1d. Sleep to wait for [sudo]'s non-echo prompt (on Unix).
			// this prevents users pass from showing up in the STDOUT.
			std::thread::sleep(std::time::Duration::from_secs(3));
			if let Err(e) = writeln!(pair.master, "{}", sudo.lock().unwrap().pass) { error!("XMRig | Sudo STDIN error: {}", e); };
			SudoState::wipe(&sudo);
		}

        // 3. Set process state
		debug!("XMRig | Setting process state...");
        let mut lock = process.lock().unwrap();
        lock.state = ProcessState::Alive;
        lock.signal = ProcessSignal::None;
        lock.start = Instant::now();
		lock.child = Some(Arc::clone(&child_pty));
		let reader = pair.master.try_clone_reader().unwrap(); // Get STDOUT/STDERR before moving the PTY
		lock.stdin = Some(pair.master);
		drop(lock);

		// 4. Spawn PTY read thread
		debug!("XMRig | Spawning PTY read thread...");
		let output_parse = Arc::clone(&process.lock().unwrap().output_parse);
		let output_pub = Arc::clone(&process.lock().unwrap().output_pub);
		thread::spawn(move || {
			Self::read_pty(output_parse, output_pub, reader, ProcessName::Xmrig);
		});
		// We don't parse anything in XMRigs output... yet.
//		let output_parse = Arc::clone(&process.lock().unwrap().output_parse);
		let output_pub = Arc::clone(&process.lock().unwrap().output_pub);

		let client: hyper::Client<hyper::client::HttpConnector> = hyper::Client::builder().build(hyper::client::HttpConnector::new());
		let start = process.lock().unwrap().start;

		// 5. Loop as watchdog
		info!("XMRig | Entering watchdog mode... woof!");
		loop {
			// Set timer
			let now = Instant::now();
			debug!("XMRig Watchdog | ----------- Start of loop -----------");

			// Check if the process secretly died without us knowing :)
			if let Ok(Some(code)) = child_pty.lock().unwrap().try_wait() {
				debug!("XMRig Watchdog | Process secretly died on us! Getting exit status...");
				let exit_status = match code.success() {
					true  => { process.lock().unwrap().state = ProcessState::Dead; "Successful" },
					false => { process.lock().unwrap().state = ProcessState::Failed; "Failed" },
				};
				let uptime = HumanTime::into_human(start.elapsed());
				info!("XMRig | Stopped ... Uptime was: [{}], Exit status: [{}]", uptime, exit_status);
				if let Err(e) = writeln!(
					gui_api.lock().unwrap().output,
					"{}\nXMRig stopped | Uptime: [{}] | Exit status: [{}]\n{}\n\n\n\n",
					HORI_CONSOLE,
					uptime,
					exit_status,
					HORI_CONSOLE
				) {
					error!("XMRig Watchdog | GUI Uptime/Exit status write failed: {}", e);
				}
				process.lock().unwrap().signal = ProcessSignal::None;
				debug!("XMRig Watchdog | Secret dead process reap OK, breaking");
				break
			}

			// Stop on [Stop/Restart] SIGNAL
			let signal = process.lock().unwrap().signal;
			if signal == ProcessSignal::Stop || signal == ProcessSignal::Restart  {
				debug!("XMRig Watchdog | Stop/Restart SIGNAL caught");
				// macOS requires [sudo] again to kill [XMRig]
				if cfg!(target_os = "macos") {
					// If we're at this point, that means the user has
					// entered their [sudo] pass again, after we wiped it.
					// So, we should be able to find it in our [Arc<Mutex<SudoState>>].
					Self::sudo_kill(child_pty.lock().unwrap().process_id().unwrap(), &sudo);
					// And... wipe it again (only if we're stopping full).
					// If we're restarting, the next start will wipe it for us.
					if signal != ProcessSignal::Restart { SudoState::wipe(&sudo); }
				} else if let Err(e) = child_pty.lock().unwrap().kill() {
					error!("XMRig Watchdog | Kill error: {}", e);
				}
				let exit_status = match child_pty.lock().unwrap().wait() {
					Ok(e) => {
						let mut process = process.lock().unwrap();
						if e.success() {
							if process.signal == ProcessSignal::Stop { process.state = ProcessState::Dead; }
							"Successful"
						} else {
							if process.signal == ProcessSignal::Stop { process.state = ProcessState::Failed; }
							"Failed"
						}
					},
					_ => {
						let mut process = process.lock().unwrap();
						if process.signal == ProcessSignal::Stop { process.state = ProcessState::Failed; }
						"Unknown Error"
					},
				};
				let uptime = HumanTime::into_human(start.elapsed());
				info!("XMRig | Stopped ... Uptime was: [{}], Exit status: [{}]", uptime, exit_status);
				if let Err(e) = writeln!(
					gui_api.lock().unwrap().output,
					"{}\nXMRig stopped | Uptime: [{}] | Exit status: [{}]\n{}\n\n\n\n",
					HORI_CONSOLE,
					uptime,
					exit_status,
					HORI_CONSOLE
				) {
					error!("XMRig Watchdog | GUI Uptime/Exit status write failed: {}", e);
				}
				let mut process = process.lock().unwrap();
				match process.signal {
					ProcessSignal::Stop    => process.signal = ProcessSignal::None,
					ProcessSignal::Restart => process.state = ProcessState::Waiting,
					_ => (),
				}
				debug!("XMRig Watchdog | Stop/Restart SIGNAL done, breaking");
				break
			}

			// Check vector of user input
			let mut lock = process.lock().unwrap();
			if !lock.input.is_empty() {
				let input = std::mem::take(&mut lock.input);
				for line in input {
					debug!("XMRig Watchdog | User input not empty, writing to STDIN: [{}]", line);
					if let Err(e) = writeln!(lock.stdin.as_mut().unwrap(), "{}", line) { error!("XMRig Watchdog | STDIN error: {}", e); };
				}
			}
			drop(lock);

			// Check if logs need resetting
			debug!("XMRig Watchdog | Attempting GUI log reset check");
			let mut lock = gui_api.lock().unwrap();
			Self::check_reset_gui_output(&mut lock.output, ProcessName::Xmrig);
			drop(lock);

			// Always update from output
			debug!("XMRig Watchdog | Starting [update_from_output()]");
			PubXmrigApi::update_from_output(&pub_api, &output_pub, start.elapsed());

			// Send an HTTP API request
			debug!("XMRig Watchdog | Attempting HTTP API request...");
			if let Ok(priv_api) = PrivXmrigApi::request_xmrig_api(client.clone(), &api_ip_port).await {
				debug!("XMRig Watchdog | HTTP API request OK, attempting [update_from_priv()]");
				PubXmrigApi::update_from_priv(&pub_api, priv_api);
			} else {
				warn!("XMRig Watchdog | Could not send HTTP API request to: {}", api_ip_port);
			}

			// Sleep (only if 900ms hasn't passed)
			let elapsed = now.elapsed().as_millis();
			// Since logic goes off if less than 1000, casting should be safe
			if elapsed < 900 {
				let sleep = (900-elapsed) as u64;
				debug!("XMRig Watchdog | END OF LOOP - Sleeping for [{}]ms...", sleep);
				std::thread::sleep(std::time::Duration::from_millis(sleep));
			} else {
				debug!("XMRig Watchdog | END OF LOOP - Not sleeping!");
			}
		}

		// 5. If loop broke, we must be done here.
		info!("XMRig Watchdog | Watchdog thread exiting... Goodbye!");
	}

	//---------------------------------------------------------------------------------------------------- The "helper"
	fn update_pub_sys_from_sysinfo(sysinfo: &sysinfo::System, pub_sys: &mut Sys, pid: &sysinfo::Pid, helper: &Helper, max_threads: usize) {
		let gupax_uptime = helper.uptime.to_string();
		let cpu = &sysinfo.cpus()[0];
		let gupax_cpu_usage = format!("{:.2}%", sysinfo.process(*pid).unwrap().cpu_usage()/(max_threads as f32));
		let gupax_memory_used_mb = HumanNumber::from_u64(sysinfo.process(*pid).unwrap().memory()/1_000_000);
		let gupax_memory_used_mb = format!("{} megabytes", gupax_memory_used_mb);
		let system_cpu_model = format!("{} ({}MHz)", cpu.brand(), cpu.frequency());
		let system_memory = {
			let used = (sysinfo.used_memory() as f64)/1_000_000_000.0;
			let total = (sysinfo.total_memory() as f64)/1_000_000_000.0;
			format!("{:.3} GB / {:.3} GB", used, total)
		};
		let system_cpu_usage = {
			let mut total: f32 = 0.0;
			for cpu in sysinfo.cpus() {
				total += cpu.cpu_usage();
			}
			format!("{:.2}%", total/(max_threads as f32))
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

	// The "helper" thread. Syncs data between threads here and the GUI.
	pub fn spawn_helper(helper: &Arc<Mutex<Self>>, mut sysinfo: sysinfo::System, pid: sysinfo::Pid, max_threads: usize) {
		// The ordering of these locks is _very_ important. They MUST be in sync with how the main GUI thread locks stuff
		// or a deadlock will occur given enough time. They will eventually both want to lock the [Arc<Mutex>] the other
		// thread is already locking. Yes, I figured this out the hard way, hence the vast amount of debug!() messages.
		// Example of different order (BAD!):
		//
		// GUI Main       -> locks [p2pool] first
		// Helper         -> locks [gui_api_p2pool] first
		// GUI Status Tab -> trys to lock [gui_api_p2pool] -> CAN'T
		// Helper         -> trys to lock [p2pool] -> CAN'T
		//
		// These two threads are now in a deadlock because both
		// are trying to access locks the other one already has.
		//
		// The locking order here must be in the same chronological
		// order as the main GUI thread (top to bottom).

		let helper = Arc::clone(helper);
		let lock = helper.lock().unwrap();
		let p2pool = Arc::clone(&lock.p2pool);
		let xmrig = Arc::clone(&lock.xmrig);
		let pub_sys = Arc::clone(&lock.pub_sys);
		let gui_api_p2pool = Arc::clone(&lock.gui_api_p2pool);
		let gui_api_xmrig = Arc::clone(&lock.gui_api_xmrig);
		let pub_api_p2pool = Arc::clone(&lock.pub_api_p2pool);
		let pub_api_xmrig = Arc::clone(&lock.pub_api_xmrig);
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
		let mut lock = helper.lock().unwrap();                                debug!("Helper | Locking (1/8) ... [helper]");
		let p2pool = p2pool.lock().unwrap();                                  debug!("Helper | Locking (2/8) ... [p2pool]");
		let xmrig = xmrig.lock().unwrap();                                    debug!("Helper | Locking (3/8) ... [xmrig]");
		let mut lock_pub_sys = pub_sys.lock().unwrap();                       debug!("Helper | Locking (4/8) ... [pub_sys]");
		let mut gui_api_p2pool = gui_api_p2pool.lock().unwrap();              debug!("Helper | Locking (5/8) ... [gui_api_p2pool]");
		let mut gui_api_xmrig = gui_api_xmrig.lock().unwrap();                debug!("Helper | Locking (6/8) ... [gui_api_xmrig]");
		let mut pub_api_p2pool = pub_api_p2pool.lock().unwrap();              debug!("Helper | Locking (7/8) ... [pub_api_p2pool]");
		let mut pub_api_xmrig = pub_api_xmrig.lock().unwrap();                debug!("Helper | Locking (8/8) ... [pub_api_xmrig]");
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

		// 2. Selectively refresh [sysinfo] for only what we need (better performance).
		sysinfo.refresh_cpu_specifics(sysinfo_cpu);                debug!("Helper | Sysinfo refresh (1/3) ... [cpu]");
		sysinfo.refresh_processes_specifics(sysinfo_processes);    debug!("Helper | Sysinfo refresh (2/3) ... [processes]");
		sysinfo.refresh_memory();                                  debug!("Helper | Sysinfo refresh (3/3) ... [memory]");
		debug!("Helper | Sysinfo OK, running [update_pub_sys_from_sysinfo()]");
		Self::update_pub_sys_from_sysinfo(&sysinfo, &mut lock_pub_sys, &pid, &lock, max_threads);

		// 3. Drop... (almost) EVERYTHING... IN REVERSE!
		drop(lock_pub_sys);     debug!("Helper | Unlocking (1/8) ... [pub_sys]");
		drop(xmrig);            debug!("Helper | Unlocking (2/8) ... [xmrig]");
		drop(p2pool);           debug!("Helper | Unlocking (3/8) ... [p2pool]");
		drop(pub_api_xmrig);    debug!("Helper | Unlocking (4/8) ... [pub_api_xmrig]");
		drop(pub_api_p2pool);   debug!("Helper | Unlocking (5/8) ... [pub_api_p2pool]");
		drop(gui_api_xmrig);    debug!("Helper | Unlocking (6/8) ... [gui_api_xmrig]");
		drop(gui_api_p2pool);   debug!("Helper | Unlocking (7/8) ... [gui_api_p2pool]");
		drop(lock);             debug!("Helper | Unlocking (8/8) ... [helper]");

		// 4. Calculate if we should sleep or not.
		// If we should sleep, how long?
		let elapsed = start.elapsed().as_millis();
		if elapsed < 1000 {
			// Casting from u128 to u64 should be safe here, because [elapsed]
			// is less than 1000, meaning it can fit into a u64 easy.
			let sleep = (1000-elapsed) as u64;
			debug!("Helper | END OF LOOP - Sleeping for [{}]ms...", sleep);
			std::thread::sleep(std::time::Duration::from_millis(sleep));
		} else {
			debug!("Helper | END OF LOOP - Not sleeping!");
		}

		// 5. End loop
		}
		});
	}
}

//---------------------------------------------------------------------------------------------------- [HumanTime]
// This converts a [std::time::Duration] into something more readable.
// Used for uptime display purposes: [7 years, 8 months, 15 days, 23 hours, 35 minutes, 1 second]
// Code taken from [https://docs.rs/humantime/] and edited to remove sub-second time, change spacing and some words.
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct HumanTime(Duration);

impl Default for HumanTime {
	fn default() -> Self {
		Self::new()
	}
}

impl HumanTime {
	pub fn new() -> HumanTime {
		HumanTime(ZERO_SECONDS)
	}

	pub fn into_human(d: Duration) -> HumanTime {
		HumanTime(d)
	}

	fn plural(f: &mut std::fmt::Formatter, started: &mut bool, name: &str, value: u64) -> std::fmt::Result {
		if value > 0 {
			if *started {
				f.write_str(", ")?;
			}
			write!(f, "{} {}", value, name)?;
			if value > 1 {
				f.write_str("s")?;
			}
			*started = true;
		}
		Ok(())
	}
}

impl std::fmt::Display for HumanTime {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let secs = self.0.as_secs();
		if secs == 0 {
			f.write_str("0 seconds")?;
			return Ok(());
		}

		let years = secs / 31_557_600;  // 365.25d
		let ydays = secs % 31_557_600;
		let months = ydays / 2_630_016;  // 30.44d
		let mdays = ydays % 2_630_016;
		let days = mdays / 86400;
		let day_secs = mdays % 86400;
		let hours = day_secs / 3600;
		let minutes = day_secs % 3600 / 60;
		let seconds = day_secs % 60;

		let started = &mut false;
		Self::plural(f, started, "year", years)?;
		Self::plural(f, started, "month", months)?;
		Self::plural(f, started, "day", days)?;
		Self::plural(f, started, "hour", hours)?;
		Self::plural(f, started, "minute", minutes)?;
		Self::plural(f, started, "second", seconds)?;
		Ok(())
	}
}

//---------------------------------------------------------------------------------------------------- [HumanNumber]
// Human readable numbers.
// Float    | [1234.57] -> [1,234]                    | Casts as u64/u128, adds comma
// Unsigned | [1234567] -> [1,234,567]                | Adds comma
// Percent  | [99.123] -> [99.12%]                    | Truncates to 2 after dot, adds percent
// Percent  | [0.001]  -> [0%]                        | Rounds down, removes redundant zeros
// Hashrate | [123.0, 311.2, null] -> [123, 311, ???] | Casts, replaces null with [???]
// CPU Load | [12.0, 11.4, null] -> [12.0, 11.4, ???] | No change, just into [String] form
#[derive(Debug, Clone)]
pub struct HumanNumber(String);

impl std::fmt::Display for HumanNumber {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(f, "{}", &self.0)
	}
}

impl HumanNumber {
	fn unknown() -> Self {
		Self("???".to_string())
	}
	fn to_percent(f: f32) -> Self {
		if f < 0.01 {
			Self("0%".to_string())
		} else {
			Self(format!("{:.2}%", f))
		}
	}
	fn from_f32(f: f32) -> Self {
		let mut buf = num_format::Buffer::new();
		buf.write_formatted(&(f as u64), &LOCALE);
		Self(buf.as_str().to_string())
	}
	fn from_f64(f: f64) -> Self {
		let mut buf = num_format::Buffer::new();
		buf.write_formatted(&(f as u128), &LOCALE);
		Self(buf.as_str().to_string())
	}
	fn from_u8(u: u8) -> Self {
		let mut buf = num_format::Buffer::new();
		buf.write_formatted(&u, &LOCALE);
		Self(buf.as_str().to_string())
	}
	fn from_u16(u: u16) -> Self {
		let mut buf = num_format::Buffer::new();
		buf.write_formatted(&u, &LOCALE);
		Self(buf.as_str().to_string())
	}
	fn from_u32(u: u32) -> Self {
		let mut buf = num_format::Buffer::new();
		buf.write_formatted(&u, &LOCALE);
		Self(buf.as_str().to_string())
	}
	fn from_u64(u: u64) -> Self {
		let mut buf = num_format::Buffer::new();
		buf.write_formatted(&u, &LOCALE);
		Self(buf.as_str().to_string())
	}
	fn from_u128(u: u128) -> Self {
		let mut buf = num_format::Buffer::new();
		buf.write_formatted(&u, &LOCALE);
		Self(buf.as_str().to_string())
	}
	fn from_hashrate(array: [Option<f32>; 3]) -> Self {
		let mut string = "[".to_string();
		let mut buf = num_format::Buffer::new();

		let mut n = 0;
		for i in array {
			match i {
				Some(f) => {
					let f = f as u128;
					buf.write_formatted(&f, &LOCALE);
					string.push_str(buf.as_str());
					string.push_str(" H/s");
				},
				None => string.push_str("??? H/s"),
			}
			if n != 2 {
				string.push_str(", ");
				n += 1;
			} else {
				string.push(']');
				break
			}
		}

		Self(string)
	}
	fn from_load(array: [Option<f32>; 3]) -> Self {
		let mut string = "[".to_string();
		let mut n = 0;
		for i in array {
			match i {
				Some(f) => string.push_str(format!("{}", f).as_str()),
				None => string.push_str("???"),
			}
			if n != 2 {
				string.push_str(", ");
				n += 1;
			} else {
				string.push(']');
				break
			}
		}
		Self(string)
	}
}

//---------------------------------------------------------------------------------------------------- Regexes
// Not to be confused with the [Regexes] struct in [main.rs], this one is meant
// for parsing the output of P2Pool and finding payouts and total XMR found.
// Why Regex instead of the standard library?
//    1. I'm already using Regex
//    2. It's insanely faster
//
// The following STDLIB implementation takes [0.003~] seconds to find all matches given a [String] with 30k lines:
//     let mut n = 0;
//     for line in P2POOL_OUTPUT.lines() {
//         if line.contains("You received a payout of [0-9].[0-9]+ XMR") { n += 1; }
//     }
//
// This regex function takes [0.0003~] seconds (10x faster):
//     let regex = Regex::new("You received a payout of [0-9].[0-9]+ XMR").unwrap();
//     let n = regex.find_iter(P2POOL_OUTPUT).count();
//
// Both are nominally fast enough where it doesn't matter too much but meh, why not use regex.
struct P2poolRegex {
	payout: regex::Regex,
	float: regex::Regex,
}

impl P2poolRegex {
	fn new() -> Self {
		Self {
			payout: regex::Regex::new("You received a payout of [0-9].[0-9]+ XMR").unwrap(),
			float: regex::Regex::new("[0-9].[0-9]+").unwrap(),
		}
	}
}

//---------------------------------------------------------------------------------------------------- [ImgP2pool]
// A static "image" of data that P2Pool started with.
// This is just a snapshot of the user data when they initially started P2Pool.
// Created by [start_p2pool()] and return to the main GUI thread where it will store it.
// No need for an [Arc<Mutex>] since the Helper thread doesn't need this information.
#[derive(Debug, Clone)]
pub struct ImgP2pool {
	pub mini: bool,        // Did the user start on the mini-chain?
	pub address: String,   // What address is the current p2pool paying out to? (This gets shortened to [4xxxxx...xxxxxx])
	pub host: String,      // What monerod are we using?
	pub rpc: String,       // What is the RPC port?
	pub zmq: String,       // What is the ZMQ port?
	pub out_peers: String, // How many out-peers?
	pub in_peers: String,  // How many in-peers?
	pub log_level: String, // What log level?
}

impl ImgP2pool {
	pub fn new() -> Self {
		Self {
			mini: true,
			address: String::new(),
			host: String::new(),
			rpc: String::new(),
			zmq: String::new(),
			out_peers: String::new(),
			in_peers: String::new(),
			log_level: String::new(),
		}
	}
}

//---------------------------------------------------------------------------------------------------- Public P2Pool API
// Helper/GUI threads both have a copy of this, Helper updates
// the GUI's version on a 1-second interval from the private data.
#[derive(Debug, Clone)]
pub struct PubP2poolApi {
	// Output
	pub output: String,
	// Uptime
	pub uptime: HumanTime,
	// These are manually parsed from the STDOUT.
	pub payouts: u128,
	pub payouts_hour: f64,
	pub payouts_day: f64,
	pub payouts_month: f64,
	pub xmr: f64,
	pub xmr_hour: f64,
	pub xmr_day: f64,
	pub xmr_month: f64,
	// The rest are serialized from the API, then turned into [HumanNumber]s
	pub hashrate_15m: HumanNumber,
	pub hashrate_1h: HumanNumber,
	pub hashrate_24h: HumanNumber,
	pub shares_found: HumanNumber,
	pub average_effort: HumanNumber,
	pub current_effort: HumanNumber,
	pub connections: HumanNumber,
}

impl Default for PubP2poolApi {
	fn default() -> Self {
		Self::new()
	}
}

impl PubP2poolApi {
	pub fn new() -> Self {
		Self {
			output: String::new(),
			uptime: HumanTime::new(),
			payouts: 0,
			payouts_hour: 0.0,
			payouts_day: 0.0,
			payouts_month: 0.0,
			xmr: 0.0,
			xmr_hour: 0.0,
			xmr_day: 0.0,
			xmr_month: 0.0,
			hashrate_15m: HumanNumber::unknown(),
			hashrate_1h: HumanNumber::unknown(),
			hashrate_24h: HumanNumber::unknown(),
			shares_found: HumanNumber::unknown(),
			average_effort: HumanNumber::unknown(),
			current_effort: HumanNumber::unknown(),
			connections: HumanNumber::unknown(),
		}
	}

	// The issue with just doing [gui_api = pub_api] is that values get overwritten.
	// This doesn't matter for any of the values EXCEPT for the output, so we must
	// manually append it instead of overwriting.
	// This is used in the "helper" thread.
	fn combine_gui_pub_api(gui_api: &mut Self, pub_api: &mut Self) {
		let output = std::mem::take(&mut gui_api.output);
		let buf = std::mem::take(&mut pub_api.output);
		*gui_api = Self {
			output,
			..std::mem::take(pub_api)
		};
		if !buf.is_empty() { gui_api.output.push_str(&buf); }
	}

	// Mutate "watchdog"'s [PubP2poolApi] with data the process output.
	fn update_from_output(public: &Arc<Mutex<Self>>, output_parse: &Arc<Mutex<String>>, output_pub: &Arc<Mutex<String>>, elapsed: std::time::Duration, regex: &P2poolRegex) {
		// 1. Take the process's current output buffer and combine it with Pub (if not empty)
		let mut output_pub = output_pub.lock().unwrap();
		if !output_pub.is_empty() {
			public.lock().unwrap().output.push_str(&std::mem::take(&mut *output_pub));
		}

		// 2. Parse the full STDOUT
		let mut output_parse = output_parse.lock().unwrap();
		let (payouts, xmr) = Self::calc_payouts_and_xmr(&output_parse, regex);
		// 3. Throw away [output_parse]
		output_parse.clear();
		drop(output_parse);
		let lock = public.lock().unwrap();
		// 4. Add to current values
		let (payouts, xmr) = (lock.payouts + payouts, lock.xmr + xmr);
		drop(lock);

		// 5. Calculate hour/day/month given elapsed time
		let elapsed_as_secs_f64 = elapsed.as_secs_f64();
		// Payouts
		let per_sec = (payouts as f64) / elapsed_as_secs_f64;
		let payouts_hour = (per_sec * 60.0) * 60.0;
		let payouts_day = payouts_hour * 24.0;
		let payouts_month = payouts_day * 30.0;
		// Total XMR
		let per_sec = xmr / elapsed_as_secs_f64;
		let xmr_hour = (per_sec * 60.0) * 60.0;
		let xmr_day = xmr_hour * 24.0;
		let xmr_month = xmr_day * 30.0;

		// 6. Mutate the struct with the new info
		let mut public = public.lock().unwrap();
		*public = Self {
			uptime: HumanTime::into_human(elapsed),
			payouts,
			xmr,
			payouts_hour,
			payouts_day,
			payouts_month,
			xmr_hour,
			xmr_day,
			xmr_month,
			..std::mem::take(&mut *public)
		};
	}

	// Mutate [PubP2poolApi] with data from a [PrivP2poolApi] and the process output.
	fn update_from_priv(public: &Arc<Mutex<Self>>, private: PrivP2poolApi) {
		// priv -> pub conversion
		let mut public = public.lock().unwrap();
		*public = Self {
			hashrate_15m: HumanNumber::from_u128(private.hashrate_15m),
			hashrate_1h: HumanNumber::from_u128(private.hashrate_1h),
			hashrate_24h: HumanNumber::from_u128(private.hashrate_24h),
			shares_found: HumanNumber::from_u128(private.shares_found),
			average_effort: HumanNumber::to_percent(private.average_effort),
			current_effort: HumanNumber::to_percent(private.current_effort),
			connections: HumanNumber::from_u16(private.connections),
			..std::mem::take(&mut *public)
		};
	}

	// Essentially greps the output for [x.xxxxxxxxxxxx XMR] where x = a number.
	// It sums each match and counts along the way, handling an error by not adding and printing to console.
	fn calc_payouts_and_xmr(output: &str, regex: &P2poolRegex) -> (u128 /* payout count */, f64 /* total xmr */) {
		let iter = regex.payout.find_iter(output);
		let mut result: f64 = 0.0;
		let mut count: u128 = 0;
		for i in iter {
			match regex.float.find(i.as_str()).unwrap().as_str().parse::<f64>() {
				Ok(num) => { result += num; count += 1; },
				Err(e)  => error!("P2Pool | Total XMR sum calculation error: [{}]", e),
			}
		}
		(count, result)
	}
}

//---------------------------------------------------------------------------------------------------- Private P2Pool API
// This is the data the "watchdog" threads mutate.
// It matches directly to P2Pool's [local/stats] JSON API file (excluding a few stats).
// P2Pool seems to initialize all stats at 0 (or 0.0), so no [Option] wrapper seems needed.
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
struct PrivP2poolApi {
	hashrate_15m: u128,
	hashrate_1h: u128,
	hashrate_24h: u128,
	shares_found: u128,
	average_effort: f32,
	current_effort: f32,
	connections: u16, // No one will have more than 65535 connections... right?
}

impl PrivP2poolApi {
	fn new() -> Self {
		Self {
			hashrate_15m: 0,
			hashrate_1h: 0,
			hashrate_24h: 0,
			shares_found: 0,
			average_effort: 0.0,
			current_effort: 0.0,
			connections: 0,
		}
	}

	// Read P2Pool's API file to a [String].
	fn read_p2pool_api(path: &std::path::PathBuf) -> Result<String, std::io::Error> {
		match std::fs::read_to_string(path) {
			Ok(s) => Ok(s),
			Err(e) => { warn!("P2Pool API | [{}] read error: {}", path.display(), e); Err(e) },
		}
	}

	// Deserialize the above [String] into a [PrivP2poolApi]
	fn str_to_priv_p2pool_api(string: &str) -> Result<Self, serde_json::Error> {
		match serde_json::from_str::<Self>(string) {
			Ok(a) => Ok(a),
			Err(e) => { warn!("P2Pool API | Could not deserialize API data: {}", e); Err(e) },
		}
	}
}

//---------------------------------------------------------------------------------------------------- [ImgXmrig]
#[derive(Debug, Clone)]
pub struct ImgXmrig {
	pub threads: String,
	pub url: String,
}

impl ImgXmrig {
	pub fn new() -> Self {
		Self {
			threads: "1".to_string(),
			url: "127.0.0.1:3333 (Local P2Pool)".to_string(),
		}
	}
}

//---------------------------------------------------------------------------------------------------- Public XMRig API
#[derive(Debug, Clone)]
pub struct PubXmrigApi {
	pub output: String,
	pub uptime: HumanTime,
	pub worker_id: String,
	pub resources: HumanNumber,
	pub hashrate: HumanNumber,
	pub pool: String,
	pub diff: HumanNumber,
	pub accepted: HumanNumber,
	pub rejected: HumanNumber,
}

impl Default for PubXmrigApi {
	fn default() -> Self {
		Self::new()
	}
}

impl PubXmrigApi {
	pub fn new() -> Self {
		Self {
			output: String::new(),
			uptime: HumanTime::new(),
			worker_id: "???".to_string(),
			resources: HumanNumber::unknown(),
			hashrate: HumanNumber::unknown(),
			pool: "???".to_string(),
			diff: HumanNumber::unknown(),
			accepted: HumanNumber::unknown(),
			rejected: HumanNumber::unknown(),
		}
	}

	fn combine_gui_pub_api(gui_api: &mut Self, pub_api: &mut Self) {
		let output = std::mem::take(&mut gui_api.output);
		let buf = std::mem::take(&mut pub_api.output);
		*gui_api = Self {
			output,
			..std::mem::take(pub_api)
		};
		if !buf.is_empty() { gui_api.output.push_str(&buf); }
	}

	// This combines the buffer from the PTY thread [output_pub]
	// with the actual [PubApiXmrig] output field.
	fn update_from_output(public: &Arc<Mutex<Self>>, output_pub: &Arc<Mutex<String>>, elapsed: std::time::Duration) {
		// 1. Take process output buffer if not empty
		let mut output_pub = output_pub.lock().unwrap();
		let mut public = public.lock().unwrap();
		// 2. Append
		if !output_pub.is_empty() {
			public.output.push_str(&std::mem::take(&mut *output_pub));
		}
		// 3. Update uptime
		public.uptime = HumanTime::into_human(elapsed);
	}

	// Formats raw private data into ready-to-print human readable version.
	fn update_from_priv(public: &Arc<Mutex<Self>>, private: PrivXmrigApi) {
		let mut public = public.lock().unwrap();
		*public = Self {
			worker_id: private.worker_id,
			resources: HumanNumber::from_load(private.resources.load_average),
			hashrate: HumanNumber::from_hashrate(private.hashrate.total),
			pool: private.connection.pool,
			diff: HumanNumber::from_u128(private.connection.diff),
			accepted: HumanNumber::from_u128(private.connection.accepted),
			rejected: HumanNumber::from_u128(private.connection.rejected),
			..std::mem::take(&mut *public)
		}
	}
}

//---------------------------------------------------------------------------------------------------- Private XMRig API
// This matches to some JSON stats in the HTTP call [summary],
// e.g: [wget -qO- localhost:18085/1/summary].
// XMRig doesn't initialize stats at 0 (or 0.0) and instead opts for [null]
// which means some elements need to be wrapped in an [Option] or else serde will [panic!].
#[derive(Debug, Serialize, Deserialize, Clone)]
struct PrivXmrigApi {
	worker_id: String,
	resources: Resources,
	connection: Connection,
	hashrate: Hashrate,
}

impl PrivXmrigApi {
	fn new() -> Self {
		Self {
			worker_id: String::new(),
			resources: Resources::new(),
			connection: Connection::new(),
			hashrate: Hashrate::new(),
		}
	}
	// Send an HTTP request to XMRig's API, serialize it into [Self] and return it
	async fn request_xmrig_api(client: hyper::Client<hyper::client::HttpConnector>, api_ip_port: &str) -> Result<Self, anyhow::Error> {
		let request = hyper::Request::builder()
			.method("GET")
			.uri("http://".to_string() + api_ip_port + XMRIG_API_URI)
			.body(hyper::Body::empty())?;
		let response = tokio::time::timeout(std::time::Duration::from_millis(500), client.request(request)).await?;
		let body = hyper::body::to_bytes(response?.body_mut()).await?;
		Ok(serde_json::from_slice::<Self>(&body)?)
	}
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
struct Resources {
	load_average: [Option<f32>; 3],
}
impl Resources {
	fn new() -> Self {
		Self {
			load_average: [Some(0.0), Some(0.0), Some(0.0)],
		}
	}
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Connection {
	pool: String,
	diff: u128,
	accepted: u128,
	rejected: u128,
}
impl Connection {
	fn new() -> Self {
		Self {
			pool: String::new(),
			diff: 0,
			accepted: 0,
			rejected: 0,
		}
	}
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
struct Hashrate {
	total: [Option<f32>; 3],
}
impl Hashrate {
	fn new() -> Self {
		Self {
			total: [Some(0.0), Some(0.0), Some(0.0)],
		}
	}
}

//---------------------------------------------------------------------------------------------------- TESTS
#[cfg(test)]
mod test {
	#[test]
	fn calc_payouts_and_xmr_from_output_p2pool() {
		use crate::helper::{PubP2poolApi,P2poolRegex};
		use std::sync::{Arc,Mutex};
		let public = Arc::new(Mutex::new(PubP2poolApi::new()));
		let output_parse = Arc::new(Mutex::new(String::from(
			r#"You received a payout of 5.000000000001 XMR in block 1111
			You received a payout of 5.000000000001 XMR in block 1112
			You received a payout of 5.000000000001 XMR in block 1113"#
		)));
		let output_pub = Arc::new(Mutex::new(String::new()));
		let elapsed = std::time::Duration::from_secs(60);
		let regex = P2poolRegex::new();
		PubP2poolApi::update_from_output(&public, &output_parse, &output_pub, elapsed, &regex);
		let public = public.lock().unwrap();
		println!("{:#?}", public);
		assert_eq!(public.payouts,       3);
		assert_eq!(public.payouts_hour,  180.0);
		assert_eq!(public.payouts_day,   4320.0);
		assert_eq!(public.payouts_month, 129600.0);
		assert_eq!(public.xmr,           15.000000000003);
		assert_eq!(public.xmr_hour,      900.00000000018);
		assert_eq!(public.xmr_day,       21600.00000000432);
		assert_eq!(public.xmr_month,     648000.0000001296);
	}

	#[test]
	fn serde_priv_p2pool_api() {
		let data =
			r#"{
				"hashrate_15m": 12,
				"hashrate_1h": 11111,
				"hashrate_24h": 468967,
				"total_hashes": 2019283840922394082390,
				"shares_found": 289037,
				"average_effort": 915.563,
				"current_effort": 129.297,
				"connections": 123,
				"incoming_connections": 96
			}"#;
		use crate::helper::PrivP2poolApi;
		let priv_api = PrivP2poolApi::str_to_priv_p2pool_api(data).unwrap();
		let json = serde_json::ser::to_string_pretty(&priv_api).unwrap();
		println!("{}", json);
		let data_after_ser =
r#"{
  "hashrate_15m": 12,
  "hashrate_1h": 11111,
  "hashrate_24h": 468967,
  "shares_found": 289037,
  "average_effort": 915.563,
  "current_effort": 129.297,
  "connections": 123
}"#;
		assert_eq!(data_after_ser, json)
	}

	#[test]
	fn serde_priv_xmrig_api() {
		let data =
		r#"{
		    "id": "6226e3sd0cd1a6es",
		    "worker_id": "hinto",
		    "uptime": 123,
		    "restricted": true,
		    "resources": {
		        "memory": {
		            "free": 123,
		            "total": 123123,
		            "resident_set_memory": 123123123
		        },
		        "load_average": [10.97, 10.58, 10.47],
		        "hardware_concurrency": 12
		    },
		    "features": ["api", "asm", "http", "hwloc", "tls", "opencl", "cuda"],
		    "results": {
		        "diff_current": 123,
		        "shares_good": 123,
		        "shares_total": 123,
		        "avg_time": 123,
		        "avg_time_ms": 123,
		        "hashes_total": 123,
		        "best": [123, 123, 123, 13, 123, 123, 123, 123, 123, 123],
		        "error_log": []
		    },
		    "algo": "rx/0",
		    "connection": {
		        "pool": "localhost:3333",
		        "ip": "127.0.0.1",
		        "uptime": 123,
		        "uptime_ms": 123,
		        "ping": 0,
		        "failures": 0,
		        "tls": null,
		        "tls-fingerprint": null,
		        "algo": "rx/0",
		        "diff": 123,
		        "accepted": 123,
		        "rejected": 123,
		        "avg_time": 123,
		        "avg_time_ms": 123,
		        "hashes_total": 123,
		        "error_log": []
		    },
		    "version": "6.18.0",
		    "kind": "miner",
		    "ua": "XMRig/6.18.0 (Linux x86_64) libuv/2.0.0-dev gcc/10.2.1",
		    "cpu": {
		        "brand": "blah blah blah",
		        "family": 1,
		        "model": 2,
		        "stepping": 0,
		        "proc_info": 123,
		        "aes": true,
		        "avx2": true,
		        "x64": true,
		        "64_bit": true,
		        "l2": 123123,
		        "l3": 123123,
		        "cores": 12,
		        "threads": 24,
		        "packages": 1,
		        "nodes": 1,
		        "backend": "hwloc/2.8.0a1-git",
		        "msr": "ryzen_19h",
		        "assembly": "ryzen",
		        "arch": "x86_64",
		        "flags": ["aes", "vaes", "avx", "avx2", "bmi2", "osxsave", "pdpe1gb", "sse2", "ssse3", "sse4.1", "popcnt", "cat_l3"]
		    },
		    "donate_level": 0,
		    "paused": false,
		    "algorithms": ["cn/1", "cn/2", "cn/r", "cn/fast", "cn/half", "cn/xao", "cn/rto", "cn/rwz", "cn/zls", "cn/double", "cn/ccx", "cn-lite/1", "cn-heavy/0", "cn-heavy/tube", "cn-heavy/xhv", "cn-pico", "cn-pico/tlo", "cn/upx2", "rx/0", "rx/wow", "rx/arq", "rx/graft", "rx/sfx", "rx/keva", "argon2/chukwa", "argon2/chukwav2", "argon2/ninja", "astrobwt", "astrobwt/v2", "ghostrider"],
		    "hashrate": {
		        "total": [111.11, 111.11, 111.11],
		        "highest": 111.11,
		        "threads": [
		            [111.11, 111.11, 111.11]
		        ]
		    },
		    "hugepages": true
		}"#;
		use crate::helper::PrivXmrigApi;
		let priv_api = serde_json::from_str::<PrivXmrigApi>(&data).unwrap();
		let json = serde_json::ser::to_string_pretty(&priv_api).unwrap();
		println!("{}", json);
		let data_after_ser =
r#"{
  "worker_id": "hinto",
  "resources": {
    "load_average": [
      10.97,
      10.58,
      10.47
    ]
  },
  "connection": {
    "pool": "localhost:3333",
    "diff": 123,
    "accepted": 123,
    "rejected": 123
  },
  "hashrate": {
    "total": [
      111.11,
      111.11,
      111.11
    ]
  }
}"#;
		assert_eq!(data_after_ser, json)
	}
}
