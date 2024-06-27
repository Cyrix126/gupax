use crate::helper::xvb::algorithm::algorithm;
use crate::helper::xvb::priv_stats::XvbPrivStats;
use crate::helper::xvb::public_stats::XvbPubStats;
use bounded_vec_deque::BoundedVecDeque;
use enclose::enc;
use log::{debug, error, info, warn};
use readable::up::Uptime;
use reqwest::Client;
use std::fmt::Write;
use std::mem;
use std::time::Duration;
use std::{
    sync::{Arc, Mutex},
    thread,
};
use tokio::spawn;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Instant};

use crate::helper::xmrig::PrivXmrigApi;
use crate::helper::xvb::rounds::round_type;
use crate::utils::constants::{XMRIG_CONFIG_URI, XVB_PUBLIC_ONLY, XVB_TIME_ALGO};
use crate::{
    helper::{ProcessSignal, ProcessState},
    utils::macros::{lock, lock2, sleep},
};

use self::nodes::XvbNode;

use super::p2pool::PubP2poolApi;
use super::xmrig::PubXmrigApi;
use super::{Helper, Process};

pub mod algorithm;
pub mod nodes;
pub mod priv_stats;
pub mod public_stats;
pub mod rounds;

impl Helper {
    // Just sets some signals for the watchdog thread to pick up on.
    pub fn stop_xvb(helper: &Arc<Mutex<Self>>) {
        info!("XvB | Attempting to stop...");
        lock2!(helper, xvb).signal = ProcessSignal::Stop;
        lock2!(helper, xvb).state = ProcessState::Middle;
    }
    pub fn restart_xvb(
        helper: &Arc<Mutex<Self>>,
        state_xvb: &crate::disk::state::Xvb,
        state_p2pool: &crate::disk::state::P2pool,
        state_xmrig: &crate::disk::state::Xmrig,
    ) {
        info!("XvB | Attempting to restart...");
        lock2!(helper, xvb).signal = ProcessSignal::Restart;
        lock2!(helper, xvb).state = ProcessState::Middle;
        let helper = helper.clone();
        let state_xvb = state_xvb.clone();
        let state_p2pool = state_p2pool.clone();
        let state_xmrig = state_xmrig.clone();
        // This thread lives to wait, start xmrig then die.
        thread::spawn(move || {
            while lock2!(helper, xvb).state != ProcessState::Waiting {
                warn!("XvB | Want to restart but process is still alive, waiting...");
                sleep!(1000);
            }
            // Ok, process is not alive, start the new one!
            info!("XvB | Old process seems dead, starting new one!");
            Self::start_xvb(&helper, &state_xvb, &state_p2pool, &state_xmrig);
        });
        info!("XMRig | Restart ... OK");
    }
    pub fn start_xvb(
        helper: &Arc<Mutex<Self>>,
        state_xvb: &crate::disk::state::Xvb,
        state_p2pool: &crate::disk::state::P2pool,
        state_xmrig: &crate::disk::state::Xmrig,
    ) {
        // 1. Clone Arc value from Helper
        // pub for writing new values that will show up on UI after helper thread update. (every seconds.)
        // gui for reading values from other thread and writing directly without waiting one second (terminal output).
        // if only gui was used, values would update on UI at different time which is not a good user experience.
        info!("XvB | cloning helper arc fields");
        // without xmrig alive, it doesn't make sense to use XvB.
        // needed to see if it is alive. For XvB process to function completely, p2pool node must be alive to check the shares in the pplns window.
        let gui_api = Arc::clone(&lock!(helper).gui_api_xvb);
        let pub_api = Arc::clone(&lock!(helper).pub_api_xvb);
        let process = Arc::clone(&lock!(helper).xvb);
        let process_p2pool = Arc::clone(&lock!(helper).p2pool);
        let gui_api_p2pool = Arc::clone(&lock!(helper).gui_api_p2pool);
        let process_xmrig = Arc::clone(&lock!(helper).xmrig);
        let gui_api_xmrig = Arc::clone(&lock!(helper).gui_api_xmrig);
        let pub_api_xmrig = Arc::clone(&lock!(helper).pub_api_xmrig);
        // Reset before printing to output.
        // Need to reset because values of stats would stay otherwise which could bring confusion even if panel is with a disabled theme.
        // at the start of a process, values must be default.
        info!(
            "XvB | resetting pub and gui but keep current node as it is updated by xmrig console."
        );
        reset_data_xvb(&pub_api, &gui_api);
        // we reset the console output because it is complete start.
        lock!(gui_api).output.clear();
        // 2. Set process state
        // XvB has not yet decided if it can continue.
        // it could fail if XvB server is offline or start partially if token/address is invalid or if p2pool or xmrig are offline.
        // this state will be received accessed by the UI directly and put the status on yellow.
        info!("XvB | Setting process state...");
        {
            let mut lock = lock!(process);
            lock.state = ProcessState::Middle;
            lock.signal = ProcessSignal::None;
            lock.start = std::time::Instant::now();
        }
        // verify if token and address are existent on XvB server

        info!("XvB | spawn watchdog");
        thread::spawn(enc!((state_xvb, state_p2pool, state_xmrig) move || {
            // thread priority, else there are issue on windows but it is also good for other OS
                Self::spawn_xvb_watchdog(
                &gui_api,
                &pub_api,
                &process,
                &state_xvb,
                &state_p2pool,
                &state_xmrig,
                &gui_api_p2pool,
                &process_p2pool,
                &gui_api_xmrig,
                &pub_api_xmrig,
                &process_xmrig,
            );
        }));
    }
    // need the helper so we can restart the thread after getting a signal not caused by a restart.
    #[allow(clippy::too_many_arguments)]
    #[tokio::main]
    async fn spawn_xvb_watchdog(
        gui_api: &Arc<Mutex<PubXvbApi>>,
        pub_api: &Arc<Mutex<PubXvbApi>>,
        process: &Arc<Mutex<Process>>,
        state_xvb: &crate::disk::state::Xvb,
        state_p2pool: &crate::disk::state::P2pool,
        state_xmrig: &crate::disk::state::Xmrig,
        gui_api_p2pool: &Arc<Mutex<PubP2poolApi>>,
        process_p2pool: &Arc<Mutex<Process>>,
        gui_api_xmrig: &Arc<Mutex<PubXmrigApi>>,
        pub_api_xmrig: &Arc<Mutex<PubXmrigApi>>,
        process_xmrig: &Arc<Mutex<Process>>,
    ) {
        // create uniq client that is going to be used for during the life of the thread.
        let client = reqwest::Client::new();

        // checks confition to start XvB, will set proper state of XvB.
        // if state is middle (everything fine here),set which xvb node could be used.
        // should wait for it, because algo needs to not be started if at least one node of XvB are not responsive.
        // if no node respond, state will be AllNodeOffline.
        // state could be offline nodes or alive at this point
        // if offlines nodes, state must not let private api and algo run. only public info, so state must not be alive.
        // in that case, a spawn would retry and change state if they are available again.
        check_conditions_for_start(
            &client,
            gui_api,
            process_p2pool,
            process_xmrig,
            process,
            state_p2pool,
            state_xvb,
        )
        .await;

        // uptime for log of signal check ?
        let start = lock!(process).start;
        // uptime of last run of algo
        let last_algorithm = Arc::new(Mutex::new(tokio::time::Instant::now()));
        // uptime of last request (public and private)
        let last_request = Arc::new(Mutex::new(tokio::time::Instant::now()));
        // algo check if his behavior must be like the first time or second time. It can reset those values to re-act like first time even if it's not the case.
        let mut first_loop = true;
        // retry will be accessed from the 1m spawn, it can influence the start of algo.
        let retry = Arc::new(Mutex::new(false));
        // time donated by algorithm. With being persistent across loop, we can construct the indicator.
        let time_donated = Arc::new(Mutex::new(0));
        // let handles;
        let handle_algo = Arc::new(Mutex::new(None));
        let handle_request = Arc::new(Mutex::new(None));
        let mut msg_retry_done = false;
        info!("XvB | Entering Process mode... ");
        loop {
            debug!("XvB Watchdog | ----------- Start of loop -----------");
            // Set timer of loop
            let start_loop = Instant::now();
            // verify if p2pool and xmrig are running, else XvB must be reloaded with another token/address to start verifying the other process.
            check_state_outcauses_xvb(
                &client,
                gui_api,
                pub_api,
                process,
                process_xmrig,
                process_p2pool,
                &mut first_loop,
                &handle_algo,
                pub_api_xmrig,
                state_p2pool,
                state_xmrig,
            )
            .await;

            // check signal
            debug!("XvB | check signal");
            if signal_interrupt(
                process,
                start.into(),
                &client,
                pub_api,
                gui_api,
                gui_api_xmrig,
                state_p2pool,
                state_xmrig,
            ) {
                info!("XvB Watchdog | Signal has stopped the loop");
                break;
            }
            // let handle_algo_c = lock!(handle_algo);
            let is_algo_started_once = lock!(handle_algo).is_some();
            let is_algo_finished = lock!(handle_algo)
                .as_ref()
                .is_some_and(|algo| algo.is_finished());
            let is_request_finished = lock!(handle_request)
                .as_ref()
                .is_some_and(|request: &JoinHandle<()>| request.is_finished())
                || lock!(handle_request).is_none();
            // Send an HTTP API request only if one minute is passed since the last request or if first loop or if algorithm need to retry or if request is finished and algo is finished or almost finished (only public and private stats). We make sure public and private stats are refreshed before doing another run of the algo.
            // We make sure algo or request are not rerun when they are not over.
            // in the case of quick refresh before new run of algo, make sure it doesn't happen multiple times.
            let last_request_expired = lock!(last_request).elapsed() >= Duration::from_secs(60);
            let should_refresh_before_next_algo = is_algo_started_once
                && lock!(last_algorithm).elapsed()
                    >= Duration::from_secs((XVB_TIME_ALGO as f32 * 0.95) as u64)
                && lock!(last_request).elapsed() >= Duration::from_secs(25);
            let process_alive = lock!(process).state == ProcessState::Alive;
            if ((last_request_expired || first_loop)
                || (*lock!(retry) || is_algo_finished || should_refresh_before_next_algo)
                    && process_alive)
                && is_request_finished
            {
                // do not wait for the request to finish so that they are retrieved at exactly one minute interval and not block the thread.
                // Private API will also use this instant if XvB is Alive.
                // first_loop is false here but could be changed to true under some conditions.
                // will send a stop signal if public stats failed or update data with new one.
                *lock!(handle_request) = Some(spawn(
                    enc!((client, pub_api, gui_api, gui_api_p2pool, gui_api_xmrig, state_xvb, state_p2pool, state_xmrig, process, last_algorithm, retry, handle_algo, time_donated, last_request) async move {
                        // needs to wait here for public stats to get private stats.
                        if last_request_expired || first_loop || should_refresh_before_next_algo {
                        XvbPubStats::update_stats(&client, &gui_api, &pub_api, &process).await;
                            *lock!(last_request) = Instant::now();
                        }
                        // private stats needs valid token and address.
                        // other stats needs everything to be alive, so just require alive here for now.
                        // maybe later differentiate to add a way to get private stats without running the algo ?
                        if lock!(process).state == ProcessState::Alive {
                            // get current share to know if we are in a round and this is a required data for algo.
                            let share = lock!(gui_api_p2pool).sidechain_shares;
                            debug!("XvB | Number of current shares: {}", share);
                        // private stats can be requested every minute or first loop or if the have almost finished.
                        if last_request_expired || first_loop || should_refresh_before_next_algo {
                            debug!("XvB Watchdog | Attempting HTTP private API request...");
                            // reload private stats, it send a signal if error that will be captured on the upper thread.
                            XvbPrivStats::update_stats(
                                &client, &state_p2pool.address, &state_xvb.token, &pub_api, &gui_api, &process,
                            )
                            .await;
                            *lock!(last_request) = Instant::now();

                            // verify in which round type we are
                            let round = round_type(share, &pub_api);
                            // refresh the round we participate in.
                            debug!("XvB | Round type: {:#?}", round);
                            lock!(pub_api).stats_priv.round_participate = round;
                            // verify if we are the winner of the current round
                            if lock!(pub_api).stats_pub.winner
                                == Helper::head_tail_of_monero_address(&state_p2pool.address).as_str()
                            {
                                lock!(pub_api).stats_priv.win_current = true
                            }
                        }
                            if (first_loop || *lock!(retry)|| is_algo_finished) && lock!(gui_api_xmrig).hashrate_raw > 0.0 && lock!(process).state == ProcessState::Alive
                            {
                                // if algo was started, it must not retry next loop.
                                *lock!(retry) = false;
                                // reset instant because algo will start.
                                *lock!(last_algorithm) = Instant::now();
                                *lock!(handle_algo) = Some(spawn(enc!((client, gui_api, gui_api_xmrig, state_xmrig, time_donated) async move {
                                    algorithm(
                                        &client,
                                        &gui_api,
                                        &gui_api_xmrig,
                                        &gui_api_p2pool,
                                        &state_xmrig.token,
                                        &state_p2pool,
                                        share,
                                        &time_donated,
                                        &state_xmrig.rig,
                                    ).await;
                                })));
                            } else {
                                // if xmrig is still at 0 HR but is alive and algorithm is skipped, recheck first 10s of xmrig inside algorithm next time (in one minute). Don't check if algo failed to start because state was not alive after getting private stats.

                                if lock!(gui_api_xmrig).hashrate_raw == 0.0 && lock!(process).state == ProcessState::Alive {
                                    *lock!(retry) = true
                                }
                            }

                        }
                    }),
                ));
            }
            // if retry is false, next time the message about waiting for xmrig HR can be shown.
            if !*lock!(retry) {
                msg_retry_done = false;
            }
            // inform user that algorithm has not yet started because it is waiting for xmrig HR.
            // show this message only once before the start of algo
            if *lock!(retry) && !msg_retry_done {
                output_console(
                    gui_api,
                    "Algorithm is waiting for 10 seconds average HR of XMRig.",
                );
                msg_retry_done = true;
            }
            // update indicator (time before switch and mining location) in private stats
            // if algo not running, second message.
            // will update countdown every second.
            // verify current node which is set by algo or circonstances (failed node).
            // verify given time set by algo and start time of current algo.
            // will run only if XvB is alive.
            // let algo time to start, so no countdown is shown.
            update_indicator_algo(
                is_algo_started_once,
                is_algo_finished,
                process,
                pub_api,
                *lock!(time_donated),
                &last_algorithm,
            );
            // first_loop is done, but maybe retry will allow the algorithm to retry again.
            if first_loop {
                first_loop = false;
            }
            // Sleep (only if 900ms hasn't passed)
            let elapsed = start_loop.elapsed().as_millis();
            // Since logic goes off if less than 1000, casting should be safe
            if elapsed < 999 {
                let sleep = (999 - elapsed) as u64;
                debug!("XvB Watchdog | END OF LOOP - Sleeping for [{}]s...", sleep);
                tokio::time::sleep(Duration::from_millis(sleep)).await;
            } else {
                debug!("XMRig Watchdog | END OF LOOP - Not sleeping!");
            }
        }
    }
}
//---------------------------------------------------------------------------------------------------- Public XvB API

#[derive(Debug, Clone, Default)]
pub struct PubXvbApi {
    pub output: String,
    pub _uptime: u64,
    pub xvb_sent_last_hour_samples: SamplesAverageHour,
    pub p2pool_sent_last_hour_samples: SamplesAverageHour,
    pub stats_pub: XvbPubStats,
    pub stats_priv: XvbPrivStats,
    // where xmrig is mining right now (or trying to).
    // will be updated by output of xmrig.
    // could also be retrieved by fetching current config.
    pub current_node: Option<XvbNode>,
}
#[derive(Debug, Clone)]
pub struct SamplesAverageHour(BoundedVecDeque<f32>);
impl Default for SamplesAverageHour {
    fn default() -> Self {
        let capacity = (3600 / XVB_TIME_ALGO) as usize;
        let mut vec = BoundedVecDeque::new(capacity);
        for _ in 0..capacity {
            vec.push_back(0.0f32);
        }
        SamplesAverageHour(vec)
    }
}

impl PubXvbApi {
    pub fn new() -> Self {
        Self::default()
    }
    // The issue with just doing [gui_api = pub_api] is that values get overwritten.
    // This doesn't matter for any of the values EXCEPT for the output,  so we must
    // manually append it instead of overwriting.
    // This is used in the "helper" thread.
    pub(super) fn combine_gui_pub_api(gui_api: &mut Self, pub_api: &mut Self) {
        let mut output = std::mem::take(&mut gui_api.output);
        let buf = std::mem::take(&mut pub_api.output);
        if !buf.is_empty() {
            output.push_str(&buf);
        }
        let runtime_mode = std::mem::take(&mut gui_api.stats_priv.runtime_mode);
        let runtime_manual_amount = std::mem::take(&mut gui_api.stats_priv.runtime_manual_amount);

        *gui_api = Self {
            output,
            stats_priv: XvbPrivStats {
                runtime_mode,
                runtime_manual_amount,
                ..pub_api.stats_priv.clone()
            },
            p2pool_sent_last_hour_samples: std::mem::take(
                &mut gui_api.p2pool_sent_last_hour_samples,
            ),
            xvb_sent_last_hour_samples: std::mem::take(&mut gui_api.xvb_sent_last_hour_samples),
            ..pub_api.clone()
        };
    }
}
async fn check_conditions_for_start(
    client: &Client,
    gui_api: &Arc<Mutex<PubXvbApi>>,
    process_p2pool: &Arc<Mutex<Process>>,
    process_xmrig: &Arc<Mutex<Process>>,
    process_xvb: &Arc<Mutex<Process>>,
    state_p2pool: &crate::disk::state::P2pool,
    state_xvb: &crate::disk::state::Xvb,
) {
    let state = if let Err(err) =
        XvbPrivStats::request_api(client, &state_p2pool.address, &state_xvb.token).await
    {
        info!("XvB | verify address and token");
        // send to console: token non existent for address on XvB server
        warn!("Xvb | Start ... Partially failed because token and associated address are not existent on XvB server: {}\n", err);
        output_console(gui_api, &format!("Token and associated address are not valid on XvB API.\nCheck if you are registered.\nError: {}", err));
        ProcessState::NotMining
    } else if lock!(process_p2pool).state != ProcessState::Alive {
        info!("XvB | verify p2pool node");
        // send to console: p2pool process is not running
        warn!("Xvb | Start ... Partially failed because P2pool instance is not ready.");
        let msg = if lock!(process_p2pool).state == ProcessState::Syncing {
            "P2pool process is not ready.\nCheck the P2pool Tab"
        } else {
            "P2pool process is not running.\nCheck the P2pool Tab"
        };
        output_console(gui_api, msg);
        ProcessState::Syncing
    } else if lock!(process_xmrig).state != ProcessState::Alive {
        // send to console: p2pool process is not running
        warn!("Xvb | Start ... Partially failed because Xmrig instance is not running.");
        // output the error to console
        output_console(
            gui_api,
            "XMRig process is not running.\nCheck the Xmrig Tab.",
        );
        ProcessState::Syncing
    } else {
        // all test passed, so it can be Alive
        // stay at middle, updateNodes will finish by syncing or offlinenodes and check_status in loop will change state accordingly.
        ProcessState::Middle
    };
    if state != ProcessState::Middle {
        // while waiting for xmrig and p2pool or getting right address/token, it can get public stats
        info!("XvB | print to console state");
        output_console(
            gui_api,
            &["XvB partially started.\n", XVB_PUBLIC_ONLY].concat(),
        );
    }
    // will update the preferred node for the first loop, even if partially started.
    lock!(process_xvb).signal = ProcessSignal::UpdateNodes(XvbNode::default());
    lock!(process_xvb).state = state;
}
#[allow(clippy::too_many_arguments)]
async fn check_state_outcauses_xvb(
    client: &Client,
    gui_api: &Arc<Mutex<PubXvbApi>>,
    pub_api: &Arc<Mutex<PubXvbApi>>,
    process: &Arc<Mutex<Process>>,
    process_xmrig: &Arc<Mutex<Process>>,
    process_p2pool: &Arc<Mutex<Process>>,
    first_loop: &mut bool,
    handle_algo: &Arc<Mutex<Option<JoinHandle<()>>>>,
    pub_api_xmrig: &Arc<Mutex<PubXmrigApi>>,
    state_p2pool: &crate::disk::state::P2pool,
    state_xmrig: &crate::disk::state::Xmrig,
) {
    // will check if the state can stay as it is.
    // p2pool and xmrig are alive if ready and running (syncing is not alive).
    let state = lock!(process).state;

    // if state is not alive, the algo should stop if it was running and p2pool should be used by xmrig.
    if let Some(handle) = lock!(handle_algo).as_ref() {
        if state != ProcessState::Alive && !handle.is_finished() {
            handle.abort();
            output_console(
                gui_api,
                "XvB process can not completely continue, algorithm of distribution of HR is stopped.",
            );
            // only update xmrig if it is alive and wasn't on p2pool already.
            if lock!(gui_api).current_node != Some(XvbNode::P2pool)
                && lock!(process_xmrig).state == ProcessState::Alive
            {
                let token_xmrig = state_xmrig.token.clone();
                let address = state_p2pool.address.clone();
                let rig = state_xmrig.rig.clone();
                spawn(enc!((client, pub_api_xmrig, gui_api) async move {

                if let Err(err) = PrivXmrigApi::update_xmrig_config(
                    &client,
                    XMRIG_CONFIG_URI,
                    &token_xmrig,
                    &XvbNode::P2pool,
                    &address,
                    &pub_api_xmrig,
                    &rig
                )
                .await
                        {
                            // show to console error about updating xmrig config
                            output_console(
                                &gui_api,
                                &format!(
                                    "Failure to update xmrig config with HTTP API.\nError: {}",
                                    err
                                ),
                            );
                        } else {
                            output_console(
                                &gui_api,
                                &format!("XvB process can not completely continue, falling back to {}", XvbNode::P2pool),
                            );
                        }

                }));
            }
        }
    }
    let is_xmrig_alive = lock!(process_xmrig).state == ProcessState::Alive;
    let is_p2pool_alive = lock!(process_p2pool).state == ProcessState::Alive;
    let p2pool_xmrig_alive = is_xmrig_alive && is_p2pool_alive;
    // if state is middle because start is not finished yet, it will not do anything.
    match state {
        ProcessState::Alive if !p2pool_xmrig_alive => {
            // they are not both alives, so state will be at syncing and data reset, state of loop also.
            info!("XvB | stopped partially because XvB Nodes are not reachable.");
            // stats must be empty put to default so the UI reflect that XvB private is not running.
            reset_data_xvb(pub_api, gui_api);
            // request from public API must be executed at next loop, do not wait for 1 minute.
            *first_loop = true;
            output_console(
                            gui_api,
                            "XvB is now partially stopped because p2pool node or xmrig came offline.\nCheck P2pool and Xmrig Tabs", 
                        );
            output_console(gui_api, XVB_PUBLIC_ONLY);
            lock!(process).state = ProcessState::Syncing;
        }
        ProcessState::Syncing if p2pool_xmrig_alive => {
            info!("XvB | started this time with p2pool and xmrig");
            // will put state on middle and update nodes
            lock!(process).state = ProcessState::Alive;
            reset_data_xvb(pub_api, gui_api);
            *first_loop = true;
            output_console(
                gui_api,
                "XvB is now started because p2pool and xmrig came online.",
            );
        }
        ProcessState::Retry => {
            debug!("XvB | Retry to get stats from https://xmrvsbeast.com in this loop if delay is done.");
            *first_loop = true;
        }
        // nothing to do, we don't want to change other state
        _ => {}
    };
}
#[allow(clippy::too_many_arguments)]
fn signal_interrupt(
    process: &Arc<Mutex<Process>>,
    start: Instant,
    client: &Client,
    pub_api: &Arc<Mutex<PubXvbApi>>,
    gui_api: &Arc<Mutex<PubXvbApi>>,
    gui_api_xmrig: &Arc<Mutex<PubXmrigApi>>,
    state_p2pool: &crate::disk::state::P2pool,
    state_xmrig: &crate::disk::state::Xmrig,
) -> bool {
    // Check SIGNAL
    // check if STOP or RESTART Signal is given.
    // if STOP, will put Signal to None, if Restart to Wait
    // in either case, will break from loop.
    let signal = lock!(process).signal;
    match signal {
        ProcessSignal::Stop => {
            debug!("P2Pool Watchdog | Stop SIGNAL caught");
            // Wait to get the exit status
            let uptime = start.elapsed();
            info!(
                "Xvb Watchdog | Stopped ... Uptime was: [{}]",
                Uptime::from(uptime)
            );
            // insert the signal into output of XvB
            // This is written directly into the GUI API, because sometimes the 900ms event loop can't catch it.
            output_console(gui_api, "\n\n\nXvB stopped\n\n\n");
            debug!("XvB Watchdog | Stop SIGNAL done, breaking");
            lock!(process).signal = ProcessSignal::None;
            lock!(process).state = ProcessState::Dead;
            // reset stats
            reset_data_xvb(pub_api, gui_api);
            return true;
        }
        ProcessSignal::Restart => {
            debug!("XvB Watchdog | Restart SIGNAL caught");
            let uptime = Uptime::from(start.elapsed());
            info!("XvB Watchdog | Stopped ... Uptime was: [{}]", uptime);
            // no output to console because service will be started with fresh output.
            debug!("XvB Watchdog | Restart SIGNAL done, breaking");
            lock!(process).state = ProcessState::Waiting;
            reset_data_xvb(pub_api, gui_api);
            return true;
        }
        ProcessSignal::UpdateNodes(node) => {
            if lock!(process).state != ProcessState::Waiting {
                warn!("received the UpdateNode signal");
                let token_xmrig = state_xmrig.token.clone();
                let rig = state_xmrig.rig.clone();
                let address = state_p2pool.address.clone();
                // check if state is alive. If it is and it is receiving such a signal, it means something a node (XvB or P2Pool) has failed.
                // if XvB, xmrig needs to be switch to the other node (both will be checked though to be sure).
                // if both XvB nodes fail after checking, process will be partially stopped and a new spawn will verify if nodes are again online and so will continue the process completely if that's the case.
                // if P2pool, the process has to stop the algo and continue partially. The process will continue completely if the confitions are met again.
                // if XvB was not alive, then if it is for XvB nodes, it will check and update preferred node and set XMRig to P2pool if that's not the case.
                // if XvB was not alive and update was for P2pool, XvB must ignore. XMRig will stop sending signals because current node will be none.
                let was_alive = lock!(process).state != ProcessState::Alive;
                // so it won't execute another signal of update nodes if it is already doing it.
                lock!(process).state = ProcessState::Waiting;
                lock!(process).signal = ProcessSignal::None;
                spawn(
                    enc!((node, process, client, gui_api, pub_api, was_alive, address, token_xmrig, gui_api_xmrig) async move {
                    match node {
                        XvbNode::NorthAmerica|XvbNode::Europe if was_alive => {
                            // a node is failing. We need to first verify if a node is available
                        XvbNode::update_fastest_node(&client, &gui_api, &pub_api, &process).await;
                            if lock!(process).state == ProcessState::OfflineNodesAll {
                                // No available nodes, so launch a process to verify periodicly.
                    sleep(Duration::from_secs(10)).await;
                    warn!("node fail, set spawn that will retry nodes and update state.");
                    while lock!(process).state == ProcessState::OfflineNodesAll {
                        // this spawn will stay alive until nodes are joignable or XvB process is stopped or failed.
                        XvbNode::update_fastest_node(&client, &pub_api, &gui_api, &process).await;
                        sleep(Duration::from_secs(10)).await;
                    }
                                
                            }
                                // a good node is found, so the next check of the loop should be good and the algo will update XMRig with the good one.

                            
                        },
                        XvbNode::NorthAmerica|XvbNode::Europe if !was_alive => {
                            // Probably a start. We don't consider XMRig using XvB nodes without algo.
                                // can update xmrig and check status of state in the same time.
                            // Need to set XMRig to P2Pool if it wasn't. XMRig should have populated this value at his start.

                if lock!(gui_api).current_node != Some(XvbNode::P2pool) {
                                spawn(enc!((client, token_xmrig, address, gui_api_xmrig, gui_api) async move{
                    if let Err(err) = PrivXmrigApi::update_xmrig_config(
                        &client,
                        XMRIG_CONFIG_URI,
                        &token_xmrig,
                        &XvbNode::P2pool,
                        &address,
                        &gui_api_xmrig,
                        &rig
                    )
                    .await {
                        output_console(
                            &gui_api,
                            &format!(
                                "Failure to update xmrig config with HTTP API.\nError: {}",
                                err
                            ),
                        );
                                }
                            }
                                ));}
            },
                        _ => {}
                } } ),
                );
            }
        }
        _ => {}
    }

    false
}
fn reset_data_xvb(pub_api: &Arc<Mutex<PubXvbApi>>, gui_api: &Arc<Mutex<PubXvbApi>>) {
    let current_node = mem::take(&mut lock!(pub_api).current_node.clone());
    let runtime_mode = mem::take(&mut lock!(gui_api).stats_priv.runtime_mode);
    let runtime_manual_amount = mem::take(&mut lock!(gui_api).stats_priv.runtime_manual_amount);

    // let output = mem::take(&mut lock!(gui_api).output);
    *lock!(pub_api) = PubXvbApi::new();
    *lock!(gui_api) = PubXvbApi::new();
    // to keep the value modified by xmrig even if xvb is dead.
    lock!(pub_api).current_node = current_node;
    // to not loose the information of runtime hero mode between restart
    lock!(gui_api).stats_priv.runtime_mode = runtime_mode;
    lock!(gui_api).stats_priv.runtime_manual_amount = runtime_manual_amount;
    // message while starting must be preserved.
    // lock!(pub_api).output = output;
}
// print date time to console output in same format than xmrig
use chrono::Local;
fn datetimeonsole() -> String {
    format!("[{}]  ", Local::now().format("%Y-%m-%d %H:%M:%S%.3f"))
}
pub fn output_console(gui_api: &Arc<Mutex<PubXvbApi>>, msg: &str) {
    if let Err(e) = writeln!(lock!(gui_api).output, "{}{msg}", datetimeonsole()) {
        error!("XvB Watchdog | GUI status write failed: {}", e);
    }
}
pub fn output_console_without_time(gui_api: &Arc<Mutex<PubXvbApi>>, msg: &str) {
    if let Err(e) = writeln!(lock!(gui_api).output, "{msg}") {
        error!("XvB Watchdog | GUI status write failed: {}", e);
    }
}
fn update_indicator_algo(
    is_algo_started_once: bool,
    is_algo_finished: bool,
    process: &Arc<Mutex<Process>>,
    pub_api: &Arc<Mutex<PubXvbApi>>,
    time_donated: u32,
    last_algorithm: &Arc<Mutex<Instant>>,
) {
    if is_algo_started_once && !is_algo_finished && lock!(process).state == ProcessState::Alive {
        let node = lock!(pub_api).current_node;
        let msg_indicator = match node {
            Some(XvbNode::P2pool) if time_donated > 0 => {
                // algo is mining on p2pool but will switch to XvB after
                // show time remaining on p2pool
                lock!(pub_api).stats_priv.time_switch_node = XVB_TIME_ALGO
                    - last_algorithm.lock().unwrap().elapsed().as_secs() as u32
                    - time_donated;
                "time until switch to mining on XvB".to_string()
            }
            _ => {
                // algo is mining on XvB or complelty mining on p2pool.
                // show remaining time before next decision of algo
                // because time of last algorithm could depass a little bit XVB_TIME_ALGO before next run, check the sub.
                lock!(pub_api).stats_priv.time_switch_node = XVB_TIME_ALGO
                    .checked_sub(last_algorithm.lock().unwrap().elapsed().as_secs() as u32)
                    .unwrap_or_default();
                "time until next decision of algorithm".to_string()
            }
        };
        lock!(pub_api).stats_priv.msg_indicator = msg_indicator;
    } else {
        // if algo is not running or process not alive
        lock!(pub_api).stats_priv.time_switch_node = 0;
        lock!(pub_api).stats_priv.msg_indicator = "Algorithm is not running".to_string();
    }
}
