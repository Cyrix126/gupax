use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use log::{debug, info, warn};
use reqwest::Client;
use tokio::time::sleep;

use crate::{
    helper::{
        p2pool::PubP2poolApi,
        xmrig::{PrivXmrigApi, PubXmrigApi},
        xvb::{
            nodes::XvbNode, output_console, output_console_without_time, priv_stats::RuntimeMode,
        },
    },
    macros::lock,
    BLOCK_PPLNS_WINDOW_MAIN, BLOCK_PPLNS_WINDOW_MINI, SECOND_PER_BLOCK_P2POOL, XMRIG_CONFIG_URI,
    XVB_BUFFER, XVB_ROUND_DONOR_MEGA_MIN_HR, XVB_ROUND_DONOR_MIN_HR, XVB_ROUND_DONOR_VIP_MIN_HR,
    XVB_ROUND_DONOR_WHALE_MIN_HR, XVB_TIME_ALGO,
};

use super::{priv_stats::RuntimeDonationLevel, PubXvbApi, SamplesAverageHour};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn algorithm(
    client: &Client,
    gui_api_xvb: &Arc<Mutex<PubXvbApi>>,
    gui_api_xmrig: &Arc<Mutex<PubXmrigApi>>,
    gui_api_p2pool: &Arc<Mutex<PubP2poolApi>>,
    token_xmrig: &str,
    state_p2pool: &crate::disk::state::P2pool,
    share: u32,
    time_donated: &Arc<Mutex<u32>>,
    rig: &str,
) {
    let mut algorithm = Algorithm::new(
        client,
        gui_api_xvb,
        gui_api_xmrig,
        gui_api_p2pool,
        token_xmrig,
        state_p2pool,
        share,
        time_donated,
        rig,
    );
    algorithm.run().await;
}

struct Algorithm<'a> {
    client: &'a Client,
    gui_api_xvb: &'a Arc<Mutex<PubXvbApi>>,
    gui_api_xmrig: &'a Arc<Mutex<PubXmrigApi>>,
    gui_api_p2pool: &'a Arc<Mutex<PubP2poolApi>>,
    token_xmrig: &'a str,
    state_p2pool: &'a crate::disk::state::P2pool,
    time_donated: &'a Arc<Mutex<u32>>,
    rig: &'a str,
    stats: Stats,
}

#[derive(Debug)]
struct Stats {
    share: u32,
    hashrate_xmrig: f32,
    target_donation_hashrate: f32,
    xvb_24h_avg: f32,
    xvb_1h_avg: f32,
    address: String,
    runtime_mode: RuntimeMode,
    runtime_donation_level: RuntimeDonationLevel,
    runtime_amount: f64,
    p2pool_total_hashrate: f32,
    avg_last_hour_hashrate: f32,
    p2pool_external_hashrate: f32,
    share_min_hashrate: f32,
    spareable_hashrate: f32,
    spared_time: u32,
}

impl<'a> Algorithm<'a> {
    fn new(
        client: &'a Client,
        gui_api_xvb: &'a Arc<Mutex<PubXvbApi>>,
        gui_api_xmrig: &'a Arc<Mutex<PubXmrigApi>>,
        gui_api_p2pool: &'a Arc<Mutex<PubP2poolApi>>,
        token_xmrig: &'a str,
        state_p2pool: &'a crate::disk::state::P2pool,
        share: u32,
        time_donated: &'a Arc<Mutex<u32>>,
        rig: &'a str,
    ) -> Self {
        info!("XvB Process | Starting Algorithm - Algorithm State:",);

        let hashrate_xmrig = {
            if lock!(gui_api_xmrig).hashrate_raw_15m > 0.0 {
                lock!(gui_api_xmrig).hashrate_raw_15m
            } else if lock!(gui_api_xmrig).hashrate_raw_1m > 0.0 {
                lock!(gui_api_xmrig).hashrate_raw_1m
            } else {
                lock!(gui_api_xmrig).hashrate_raw
            }
        };

        let address = state_p2pool.address.clone();

        let xvb_24h_avg = lock!(gui_api_xvb).stats_priv.donor_24hr_avg * 1000.0;
        let xvb_1h_avg = lock!(gui_api_xvb).stats_priv.donor_1hr_avg * 1000.0;

        let runtime_mode = lock!(gui_api_xvb).stats_priv.runtime_mode.clone();
        let runtime_donation_level = lock!(gui_api_xvb)
            .stats_priv
            .runtime_manual_donation_level
            .clone();
        let runtime_amount = lock!(gui_api_xvb).stats_priv.runtime_manual_amount.clone();

        let p2pool_total_hashrate = lock!(gui_api_p2pool).sidechain_ehr;

        let avg_last_hour_hashrate =
            Self::calc_last_hour_avg_hash_rate(&lock!(gui_api_xvb).p2pool_sent_last_hour_samples);
        let mut p2pool_external_hashrate = p2pool_total_hashrate - avg_last_hour_hashrate;
        if p2pool_external_hashrate < 0.0 {
            p2pool_external_hashrate = 0.0;
        }

        let mut share_min_hashrate = Self::minimum_hashrate_share(
            lock!(gui_api_p2pool).p2pool_difficulty_u64,
            state_p2pool.mini,
            p2pool_external_hashrate,
        );
        if share_min_hashrate.is_sign_negative() {
            info!("XvB Process | if minimum HR is negative, it is 0.");
            share_min_hashrate = 0.0;
        }

        let spareable_hashrate = hashrate_xmrig - share_min_hashrate;

        // TODO consider printing algorithm stats instead of spreadout print statements
        let stats = Stats {
            share,
            hashrate_xmrig,
            xvb_24h_avg,
            xvb_1h_avg,
            address,
            target_donation_hashrate: f32::default(),
            runtime_mode,
            runtime_donation_level,
            runtime_amount,
            p2pool_total_hashrate,
            avg_last_hour_hashrate,
            p2pool_external_hashrate,
            share_min_hashrate,
            spareable_hashrate,
            spared_time: u32::default(),
        };

        let mut new_instace = Self {
            client,
            gui_api_xvb,
            gui_api_xmrig,
            gui_api_p2pool,
            token_xmrig,
            state_p2pool,
            time_donated,
            rig,
            stats,
        };

        new_instace.stats.target_donation_hashrate = new_instace.get_target_donation_hashrate();
        new_instace.stats.spared_time = ((new_instace.stats.target_donation_hashrate as u32)
            / (new_instace.stats.hashrate_xmrig as u32))
            * XVB_TIME_ALGO;

        info!("{:#?}", new_instace.stats);

        new_instace
    }

    fn is_share_fulfilled(&self) -> bool {
        self.stats.share > 0
    }

    fn is_xvb_24h_fulfilled(&self) -> bool {
        self.stats.xvb_24h_avg > self.stats.target_donation_hashrate
    }

    fn xvb_1h_fulfilled(&self) -> bool {
        self.stats.xvb_1h_avg > self.stats.target_donation_hashrate
    }

    async fn mine_p2pool(&self) {
        if lock!(self.gui_api_xvb).current_node != Some(XvbNode::P2pool) {
            info!("Xvb Process | request xmrig to mine on p2pool");

            if let Err(err) = PrivXmrigApi::update_xmrig_config(
                self.client,
                XMRIG_CONFIG_URI,
                self.token_xmrig,
                &XvbNode::P2pool,
                &self.stats.address,
                self.gui_api_xmrig,
                self.rig,
            )
            .await
            {
                warn!("Xvb Process | Failed request HTTP API Xmrig");
                output_console(
                    self.gui_api_xvb,
                    &format!(
                        "Failure to update xmrig config with HTTP API.\nError: {}",
                        err
                    ),
                );
            }
        }
        output_console(self.gui_api_xvb, "No share in the current PPLNS Window !");
        output_console(
            self.gui_api_xvb,
            "Mining on P2pool for the next ten minutes.",
        );
        sleep(Duration::from_secs(XVB_TIME_ALGO.into())).await;
        lock!(self.gui_api_xvb)
            .p2pool_sent_last_hour_samples
            .0
            .push_back(lock!(self.gui_api_xmrig).hashrate_raw_15m);
        lock!(self.gui_api_xvb)
            .xvb_sent_last_hour_samples
            .0
            .push_back(0.0);
    }

    async fn mine_xvb(&self) {
        let node = lock!(self.gui_api_xvb).stats_priv.node;

        debug!("Xvb Process | request xmrig to mine on XvB");
        if lock!(self.gui_api_xvb).current_node.is_none()
            || lock!(self.gui_api_xvb)
                .current_node
                .as_ref()
                .is_some_and(|n| n == &XvbNode::P2pool)
        {
            if let Err(err) = PrivXmrigApi::update_xmrig_config(
                self.client,
                XMRIG_CONFIG_URI,
                self.token_xmrig,
                &node,
                &self.stats.address,
                self.gui_api_xmrig,
                self.rig,
            )
            .await
            {
                // show to console error about updating xmrig config
                warn!("Xvb Process | Failed request HTTP API Xmrig");
                output_console(
                    self.gui_api_xvb,
                    &format!(
                        "Failure to update xmrig config with HTTP API.\nError: {}",
                        err
                    ),
                );
            } else {
                debug!("Xvb Process | mining on XvB pool");
            }
        }
        sleep(Duration::from_secs(XVB_TIME_ALGO.into())).await;

        lock!(self.gui_api_xvb)
            .p2pool_sent_last_hour_samples
            .0
            .push_back(lock!(self.gui_api_xmrig).hashrate_raw_15m);
        lock!(self.gui_api_xvb)
            .xvb_sent_last_hour_samples
            .0
            .push_back(0.0);
    }

    async fn sleep_then_update_node_xmrig(&self) {
        let node = lock!(self.gui_api_xvb).stats_priv.node;
        debug!(
            "Xvb Process | algo sleep for {} while mining on P2pool",
            XVB_TIME_ALGO - self.stats.spared_time
        );
        sleep(Duration::from_secs(
            (XVB_TIME_ALGO - self.stats.spared_time).into(),
        ))
        .await;

        // only update xmrig config if it is actually mining.
        debug!("Xvb Process | request xmrig to mine on XvB");
        if lock!(self.gui_api_xvb).current_node.is_none()
            || lock!(self.gui_api_xvb)
                .current_node
                .as_ref()
                .is_some_and(|n| n == &XvbNode::P2pool)
        {
            if let Err(err) = PrivXmrigApi::update_xmrig_config(
                self.client,
                XMRIG_CONFIG_URI,
                self.token_xmrig,
                &node,
                &self.stats.address,
                self.gui_api_xmrig,
                self.rig,
            )
            .await
            {
                // show to console error about updating xmrig config
                warn!("Xvb Process | Failed request HTTP API Xmrig");
                output_console(
                    self.gui_api_xvb,
                    &format!(
                        "Failure to update xmrig config with HTTP API.\nError: {}",
                        err
                    ),
                );
            } else {
                debug!("Xvb Process | mining on XvB pool");
            }
        }
        // will not quit the process until it is really done.
        // xvb process watch this algo handle to see if process is finished or not.
        sleep(Duration::from_secs(self.stats.spared_time.into())).await;
    }

    fn get_target_donation_hashrate(&self) -> f32 {
        match self.stats.runtime_mode {
            RuntimeMode::Auto => self.get_auto_mode_target_donation_hashrate(),
            RuntimeMode::Hero => self.get_hero_mode_target_donation_hashrate(),
            RuntimeMode::ManualXvb => self.stats.runtime_amount as f32,
            RuntimeMode::ManualP2pool => {
                (XVB_TIME_ALGO as f32) - (self.stats.runtime_amount as f32)
            }
            RuntimeMode::ManualDonationLevel => self.stats.runtime_donation_level.get_hashrate(),
        }
    }

    fn get_auto_mode_target_donation_hashrate(&self) -> f32 {
        // TODO fix wrong target hashrate being selected
        // TODO consider using xvb_24h_avg for calculations
        // TODO consider using dynamic buffer size buffer gets smaller as gupaxx runs for longer to provide more stability

        let donation_level = match self.stats.spareable_hashrate {
            x if x > (XVB_ROUND_DONOR_MIN_HR as f32) => Some(RuntimeDonationLevel::Donor),
            x if x > (XVB_ROUND_DONOR_VIP_MIN_HR as f32) => Some(RuntimeDonationLevel::DonorVIP),
            x if x > (XVB_ROUND_DONOR_WHALE_MIN_HR as f32) => {
                Some(RuntimeDonationLevel::DonorWhale)
            }
            x if x > (XVB_ROUND_DONOR_MEGA_MIN_HR as f32) => Some(RuntimeDonationLevel::DonorMega),
            _ => None,
        };

        if let Some(level) = donation_level {
            level.get_hashrate()
        } else {
            0.0
        }
    }

    fn get_hero_mode_target_donation_hashrate(&self) -> f32 {
        // TODO improve selection method
        // TODO consider using a large buffer size
        // TODO consider manually setting the share count to aim for on hero mode

        self.stats.spareable_hashrate
    }

    // push new value into samples before executing this calcul
    fn calc_last_hour_avg_hash_rate(samples: &SamplesAverageHour) -> f32 {
        samples.0.iter().sum::<f32>() / samples.0.len() as f32
    }

    fn minimum_hashrate_share(difficulty: u64, mini: bool, p2pool_external_hashrate: f32) -> f32 {
        let pws = if mini {
            BLOCK_PPLNS_WINDOW_MINI
        } else {
            BLOCK_PPLNS_WINDOW_MAIN
        };
        let minimum_hr = ((difficulty / (pws * SECOND_PER_BLOCK_P2POOL)) as f32 * XVB_BUFFER)
            - p2pool_external_hashrate;
        info!("XvB Process | (difficulty / (window pplns blocks * seconds per p2pool block) * BUFFER) - outside HR = minimum HR to keep a share\n({difficulty} / ({pws} * {SECOND_PER_BLOCK_P2POOL}) * {XVB_BUFFER}) - {p2pool_external_hashrate} = {minimum_hr}");
        minimum_hr
    }

    async fn run(&mut self) {
        // TODO add console output for each step

        if self.is_share_fulfilled() && self.is_xvb_24h_fulfilled() {
            output_console(
                self.gui_api_xvb,
                "There is a share in p2pool and 24H avg XvB is achieved.",
            );
            output_console(self.gui_api_xvb, "Calculating donation time for XvB...");

            *lock!(self.time_donated) = self.stats.spared_time;
            self.mine_p2pool().await;
            self.sleep_then_update_node_xmrig().await;
        } else if self.is_share_fulfilled() {
            output_console(self.gui_api_xvb, "24H avg XvB target not achieved.");
            output_console(self.gui_api_xvb, "Sending all hashrate to XvB");

            *lock!(self.time_donated) = XVB_TIME_ALGO;
            self.mine_xvb().await
        } else {
            output_console(self.gui_api_xvb, "There are no shares in p2pool");
            output_console(self.gui_api_xvb, "Sending all hashrate to p2pool");

            self.mine_p2pool().await
        }

        output_console(self.gui_api_xvb, "")
    }
}
