use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use derive_more::Display;
use log::{error, info, warn};
use reqwest::Client;
use tokio::spawn;

use crate::{
    components::node::{GetInfo, TIMEOUT_NODE_PING},
    helper::{xvb::output_console, Process, ProcessState},
    macros::lock,
    GUPAX_VERSION_UNDERSCORE, XVB_NODE_EU, XVB_NODE_NA, XVB_NODE_PORT, XVB_NODE_RPC,
};

use super::PubXvbApi;
#[derive(Copy, Clone, Debug, Default, PartialEq, Display)]
pub enum XvbNode {
    #[display(fmt = "XvB North America Node")]
    NorthAmerica,
    #[default]
    #[display(fmt = "XvB European Node")]
    Europe,
    #[display(fmt = "Local P2pool")]
    P2pool,
}
impl XvbNode {
    pub fn url(&self) -> String {
        match self {
            Self::NorthAmerica => String::from(XVB_NODE_NA),
            Self::Europe => String::from(XVB_NODE_EU),
            Self::P2pool => String::from("127.0.0.1"),
        }
    }
    pub fn port(&self) -> String {
        match self {
            Self::NorthAmerica | Self::Europe => String::from(XVB_NODE_PORT),
            Self::P2pool => String::from("3333"),
        }
    }
    pub fn user(&self, address: &str) -> String {
        match self {
            Self::NorthAmerica => address.chars().take(8).collect(),
            Self::Europe => address.chars().take(8).collect(),
            Self::P2pool => GUPAX_VERSION_UNDERSCORE.to_string(),
        }
    }
    pub fn tls(&self) -> bool {
        match self {
            Self::NorthAmerica => true,
            Self::Europe => true,
            Self::P2pool => false,
        }
    }
    pub fn keepalive(&self) -> bool {
        match self {
            Self::NorthAmerica => true,
            Self::Europe => true,
            Self::P2pool => false,
        }
    }

    pub async fn update_fastest_node(
        client: &Client,
        pub_api_xvb: &Arc<Mutex<PubXvbApi>>,
        gui_api_xvb: &Arc<Mutex<PubXvbApi>>,
        process_xvb: &Arc<Mutex<Process>>,
    ) {
        let client_eu = client.clone();
        let client_na = client.clone();
        // two spawn to ping the two nodes in parallel and not one after the other.
        let ms_eu = spawn(async move { XvbNode::ping(&XvbNode::Europe.url(), &client_eu).await });
        let ms_na =
            spawn(async move { XvbNode::ping(&XvbNode::NorthAmerica.url(), &client_na).await });
        let node = if let Ok(ms_eu) = ms_eu.await {
            if let Ok(ms_na) = ms_na.await {
                // if two nodes are up, compare ping latency and return fastest.
                if ms_na != TIMEOUT_NODE_PING && ms_eu != TIMEOUT_NODE_PING {
                    if ms_na < ms_eu {
                        XvbNode::NorthAmerica
                    } else {
                        XvbNode::Europe
                    }
                } else if ms_na != TIMEOUT_NODE_PING && ms_eu == TIMEOUT_NODE_PING {
                    // if only na is online, return it.
                    XvbNode::NorthAmerica
                } else if ms_na == TIMEOUT_NODE_PING && ms_eu != TIMEOUT_NODE_PING {
                    // if only eu is online, return it.
                    XvbNode::Europe
                } else {
                    // if P2pool is returned, it means none of the two nodes are available.
                    XvbNode::P2pool
                }
            } else {
                error!("ping has failed !");
                XvbNode::P2pool
            }
        } else {
            error!("ping has failed !");
            XvbNode::P2pool
        };
        if node == XvbNode::P2pool {
            // if both nodes are dead, then the state of the process must be NodesOffline
            info!("XvB node ping, all offline or ping failed, switching back to local p2pool",);
            output_console(
                gui_api_xvb,
                "XvB node ping, all offline or ping failed, switching back to local p2pool",
            );
            lock!(process_xvb).state = ProcessState::OfflineNodesAll;
        } else {
            // if node is up and because update_fastest is used only if token/address is valid, it means XvB process is Alive.
            info!("XvB node ping, both online and best is {}", node.url());
            output_console(
                gui_api_xvb,
                &format!("XvB node ping, {} is selected as the fastest.", node),
            );
            info!("ProcessState to Syncing after finding joinable node");
            // could be used by xmrig who signal that a node is not joignable
            // or by the start of xvb
            // next iteration of the loop of XvB process will verify if all conditions are met to be alive.
            if lock!(process_xvb).state != ProcessState::Syncing
                && lock!(process_xvb).state != ProcessState::Retry
            {
                lock!(process_xvb).state = ProcessState::Syncing;
            }
        }
        lock!(pub_api_xvb).stats_priv.node = node;
    }
    async fn ping(ip: &str, client: &Client) -> u128 {
        let request = client
            .post("http://".to_string() + ip + ":" + XVB_NODE_RPC + "/json_rpc")
            .body(r#"{"jsonrpc":"2.0","id":"0","method":"get_info"}"#);
        let ms;
        let now = Instant::now();
        match tokio::time::timeout(Duration::from_secs(8), request.send()).await {
            Ok(Ok(json_rpc)) => {
                // Attempt to convert to JSON-RPC.
                match json_rpc.bytes().await {
                    Ok(b) => match serde_json::from_slice::<GetInfo<'_>>(&b) {
                        Ok(rpc) => {
                            if rpc.result.mainnet && rpc.result.synchronized {
                                ms = now.elapsed().as_millis();
                            } else {
                                ms = TIMEOUT_NODE_PING;
                                warn!("Ping | {ip} responded with valid get_info but is not in sync, remove this node!");
                            }
                        }
                        _ => {
                            ms = TIMEOUT_NODE_PING;
                            warn!("Ping | {ip} responded but with invalid get_info, remove this node!");
                        }
                    },
                    _ => ms = TIMEOUT_NODE_PING,
                };
            }
            _ => ms = TIMEOUT_NODE_PING,
        };
        ms
    }
}
