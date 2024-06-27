#[cfg(test)]
mod test {

    use crate::helper::{
        p2pool::{PrivP2poolLocalApi, PrivP2poolNetworkApi},
        xvb::{
            algorithm::calcul_donated_time, priv_stats::RuntimeDonationLevel,
            priv_stats::RuntimeMode, rounds::round_type,
        },
        Helper, Process, ProcessName, ProcessState,
    };

    #[test]
    fn get_current_shares() {
        let stdout = "
statusfromgupaxx
2024-03-25 21:31:21.7919 SideChain status
Monero node               = node2.monerodevs.org:18089:ZMQ:18084 (37.187.74.171)
Main chain height         = 3113042
Main chain hashrate       = 1.985 GH/s
Side chain ID             = mini
Side chain height         = 7230432
Side chain hashrate       = 8.925 MH/s
PPLNS window              = 2160 blocks (+79 uncles, 0 orphans)
PPLNS window duration     = 6h 9m 46s
Your wallet address       = 4A5Dwt2qKwKEQrZfo4aBkSNtvDDAzSFbAJcyFkdW5RwDh9U4WgeZrgKT4hUoE2gv8h6NmsNMTyjsEL8eSLMbABds5rYFWnw
Your shares               = 0 blocks (+0 uncles, 0 orphans)
Block reward share        = 0.000% (0.000000000000 XMR)
2024-03-25 21:31:21.7920 StratumServer status
Hashrate (15m est) = 0 H/s
Hashrate (1h  est) = 0 H/s
Hashrate (24h est) = 0 H/s
Total hashes       = 0
Shares found       = 0
Average effort     = 0.000%
Current effort     = 0.000%
Connections        = 0 (0 incoming)
2024-03-25 21:31:21.7920 P2PServer status
Connections    = 10 (0 incoming)
Peer list size = 1209
Uptime         = 0h 2m 4s
".lines();
        let mut shares = 1;
        let mut status_output = false;
        for line in stdout {
            // if command status is sent by gupaxx process and not the user, forward it only to update_from_status method.
            // 25 lines after the command are the result of status, with last line finishing by update.
            if line.contains("statusfromgupaxx") {
                status_output = true;
                continue;
            }
            if status_output {
                if line.contains("Your shares") {
                    // update sidechain shares
                    shares = line.split_once('=').expect("should be = at Your Share, maybe new version of p2pool has different output for status command ?").1.split_once("blocks").expect("should be a 'blocks' at Your Share, maybe new version of p2pool has different output for status command ?").0.trim().parse::<u32>().expect("this should be the number of share");
                }
                if line.contains("Uptime") {
                    // end of status
                    status_output = false;
                }
                continue;
            }
        }
        assert_eq!(shares, 0);
    }
    #[test]
    fn reset_gui_output() {
        let max = crate::helper::GUI_OUTPUT_LEEWAY;
        let mut string = String::with_capacity(max);
        for _ in 0..=max {
            string.push('0');
        }
        Helper::check_reset_gui_output(&mut string, ProcessName::P2pool);
        // Some text gets added, so just check for less than 500 bytes.
        assert!(string.len() < 500);
    }

    #[test]
    fn combine_gui_pub_p2pool_api() {
        use crate::helper::PubP2poolApi;
        let mut gui_api = PubP2poolApi::new();
        let mut pub_api = PubP2poolApi::new();
        pub_api.payouts = 1;
        pub_api.payouts_hour = 2.0;
        pub_api.payouts_day = 3.0;
        pub_api.payouts_month = 4.0;
        pub_api.xmr = 1.0;
        pub_api.xmr_hour = 2.0;
        pub_api.xmr_day = 3.0;
        pub_api.xmr_month = 4.0;
        println!("BEFORE - GUI_API: {:#?}\nPUB_API: {:#?}", gui_api, pub_api);
        assert_ne!(gui_api, pub_api);
        PubP2poolApi::combine_gui_pub_api(&mut gui_api, &mut pub_api);
        println!("AFTER - GUI_API: {:#?}\nPUB_API: {:#?}", gui_api, pub_api);
        assert_eq!(gui_api, pub_api);
        pub_api.xmr = 2.0;
        PubP2poolApi::combine_gui_pub_api(&mut gui_api, &mut pub_api);
        assert_eq!(gui_api, pub_api);
        assert_eq!(gui_api.xmr, 2.0);
        assert_eq!(pub_api.xmr, 2.0);
    }

    #[test]
    fn calc_payouts_and_xmr_from_output_p2pool() {
        use crate::helper::PubP2poolApi;
        use std::sync::{Arc, Mutex};
        let public = Arc::new(Mutex::new(PubP2poolApi::new()));
        let output_parse = Arc::new(Mutex::new(String::from(
            r#"payout of 5.000000000001 XMR in block 1111
			payout of 5.000000000001 XMR in block 1112
			payout of 5.000000000001 XMR in block 1113"#,
        )));
        let output_pub = Arc::new(Mutex::new(String::new()));
        let elapsed = std::time::Duration::from_secs(60);
        let process = Arc::new(Mutex::new(Process::new(
            ProcessName::P2pool,
            "".to_string(),
            PathBuf::new(),
        )));
        PubP2poolApi::update_from_output(&public, &output_parse, &output_pub, elapsed, &process);
        let public = public.lock().unwrap();
        println!("{:#?}", public);
        assert_eq!(public.payouts, 3);
        assert_eq!(public.payouts_hour, 180.0);
        assert_eq!(public.payouts_day, 4320.0);
        assert_eq!(public.payouts_month, 129600.0);
        assert_eq!(public.xmr, 15.000000000003);
        assert_eq!(public.xmr_hour, 900.00000000018);
        assert_eq!(public.xmr_day, 21600.00000000432);
        assert_eq!(public.xmr_month, 648000.0000001296);
    }

    #[test]
    fn set_p2pool_synchronized() {
        use crate::helper::PubP2poolApi;
        use std::sync::{Arc, Mutex};
        let public = Arc::new(Mutex::new(PubP2poolApi::new()));
        let output_parse = Arc::new(Mutex::new(String::from(
            r#"payout of 5.000000000001 XMR in block 1111
			NOTICE  2021-12-27 21:42:17.2008 SideChain SYNCHRONIZED
			payout of 5.000000000001 XMR in block 1113"#,
        )));
        let output_pub = Arc::new(Mutex::new(String::new()));
        let elapsed = std::time::Duration::from_secs(60);
        let process = Arc::new(Mutex::new(Process::new(
            ProcessName::P2pool,
            "".to_string(),
            PathBuf::new(),
        )));

        // It only gets checked if we're `Syncing`.
        process.lock().unwrap().state = ProcessState::Syncing;
        PubP2poolApi::update_from_output(&public, &output_parse, &output_pub, elapsed, &process);
        println!("{:#?}", process);
        assert!(process.lock().unwrap().state == ProcessState::Alive);
    }

    #[test]
    fn p2pool_synchronized_false_positive() {
        use crate::helper::PubP2poolApi;
        use std::sync::{Arc, Mutex};
        let public = Arc::new(Mutex::new(PubP2poolApi::new()));

        // The SideChain that is "SYNCHRONIZED" in this output is
        // probably not main/mini, but the sidechain started on height 1,
        // so this should _not_ trigger alive state.
        let output_parse = Arc::new(Mutex::new(String::from(
            r#"payout of 5.000000000001 XMR in block 1111
			SideChain new chain tip: next height = 1
			NOTICE  2021-12-27 21:42:17.2008 SideChain SYNCHRONIZED
			payout of 5.000000000001 XMR in block 1113"#,
        )));
        let output_pub = Arc::new(Mutex::new(String::new()));
        let elapsed = std::time::Duration::from_secs(60);
        let process = Arc::new(Mutex::new(Process::new(
            ProcessName::P2pool,
            "".to_string(),
            PathBuf::new(),
        )));

        // It only gets checked if we're `Syncing`.
        process.lock().unwrap().state = ProcessState::Syncing;
        PubP2poolApi::update_from_output(&public, &output_parse, &output_pub, elapsed, &process);
        println!("{:#?}", process);
        assert!(process.lock().unwrap().state == ProcessState::Syncing); // still syncing
    }

    #[test]
    fn p2pool_synchronized_double_synchronized() {
        use crate::helper::PubP2poolApi;
        use std::sync::{Arc, Mutex};
        let public = Arc::new(Mutex::new(PubP2poolApi::new()));

        // The 1st SideChain that is "SYNCHRONIZED" in this output is
        // the sidechain started on height 1, but there is another one
        // which means the real main/mini is probably synced,
        // so this _should_ trigger alive state.
        let output_parse = Arc::new(Mutex::new(String::from(
            r#"payout of 5.000000000001 XMR in block 1111
			SideChain new chain tip: next height = 1
			NOTICE  2021-12-27 21:42:17.2008 SideChain SYNCHRONIZED
			payout of 5.000000000001 XMR in block 1113
			NOTICE  2021-12-27 21:42:17.2100 SideChain SYNCHRONIZED"#,
        )));
        let output_pub = Arc::new(Mutex::new(String::new()));
        let elapsed = std::time::Duration::from_secs(60);
        let process = Arc::new(Mutex::new(Process::new(
            ProcessName::P2pool,
            "".to_string(),
            PathBuf::new(),
        )));

        // It only gets checked if we're `Syncing`.
        process.lock().unwrap().state = ProcessState::Syncing;
        PubP2poolApi::update_from_output(&public, &output_parse, &output_pub, elapsed, &process);
        println!("{:#?}", process);
        assert!(process.lock().unwrap().state == ProcessState::Alive);
    }

    #[test]
    fn update_pub_p2pool_from_local_network_pool() {
        use crate::helper::p2pool::PoolStatistics;
        use crate::helper::p2pool::PrivP2poolLocalApi;
        use crate::helper::p2pool::PrivP2poolNetworkApi;
        use crate::helper::p2pool::PrivP2poolPoolApi;
        use crate::helper::PubP2poolApi;
        use std::sync::{Arc, Mutex};
        let public = Arc::new(Mutex::new(PubP2poolApi::new()));
        let local = PrivP2poolLocalApi {
            hashrate_15m: 10_000,
            hashrate_1h: 20_000,
            hashrate_24h: 30_000,
            shares_found: 1000,
            average_effort: 100.000,
            current_effort: 200.000,
            connections: 1234,
        };
        let network = PrivP2poolNetworkApi {
            difficulty: 300_000_000_000,
            hash: "asdf".to_string(),
            height: 1234,
            reward: 2345,
            timestamp: 3456,
        };
        let pool = PrivP2poolPoolApi {
            pool_statistics: PoolStatistics {
                hashRate: 1_000_000, // 1 MH/s
                miners: 1_000,
            },
        };
        // Update Local
        PubP2poolApi::update_from_local(&public, local);
        let p = public.lock().unwrap();
        println!("AFTER LOCAL: {:#?}", p);
        assert_eq!(p.hashrate_15m.to_string(), "10,000");
        assert_eq!(p.hashrate_1h.to_string(), "20,000");
        assert_eq!(p.hashrate_24h.to_string(), "30,000");
        assert_eq!(
            p.shares_found.expect("the value is set").to_string(),
            "1000"
        );
        assert_eq!(p.average_effort.to_string(), "100.00%");
        assert_eq!(p.current_effort.to_string(), "200.00%");
        assert_eq!(p.connections.to_string(), "1,234");
        assert_eq!(p.user_p2pool_hashrate_u64, 20000);
        drop(p);
        // Update Network + Pool
        PubP2poolApi::update_from_network_pool(&public, network, pool);
        let p = public.lock().unwrap();
        println!("AFTER NETWORK+POOL: {:#?}", p);
        assert_eq!(p.monero_difficulty.to_string(), "300,000,000,000");
        assert_eq!(p.monero_hashrate.to_string(), "2.500 GH/s");
        assert_eq!(p.hash.to_string(), "asdf");
        assert_eq!(p.height.to_string(), "1,234");
        assert_eq!(p.reward.to_u64(), 2345);
        assert_eq!(p.p2pool_difficulty.to_string(), "10,000,000");
        assert_eq!(p.p2pool_hashrate.to_string(), "1.000 MH/s");
        assert_eq!(p.miners.to_string(), "1,000");
        assert_eq!(
            p.solo_block_mean.to_string(),
            "5 months, 21 days, 9 hours, 52 minutes"
        );
        assert_eq!(
            p.p2pool_block_mean.to_string(),
            "3 days, 11 hours, 20 minutes"
        );
        assert_eq!(p.p2pool_share_mean.to_string(), "8 minutes, 20 seconds");
        assert_eq!(p.p2pool_percent.to_string(), "0.040000%");
        assert_eq!(p.user_p2pool_percent.to_string(), "2.000000%");
        assert_eq!(p.user_monero_percent.to_string(), "0.000800%");
        drop(p);
    }

    #[test]
    fn set_xmrig_mining() {
        use crate::helper::PubXmrigApi;
        use std::sync::{Arc, Mutex};
        let public = Arc::new(Mutex::new(PubXmrigApi::new()));
        let output_parse = Arc::new(Mutex::new(String::from(
            "[2022-02-12 12:49:30.311]  net      no active pools, stop mining",
        )));
        let output_pub = Arc::new(Mutex::new(String::new()));
        let elapsed = std::time::Duration::from_secs(60);
        let process = Arc::new(Mutex::new(Process::new(
            ProcessName::Xmrig,
            "".to_string(),
            PathBuf::new(),
        )));

        process.lock().unwrap().state = ProcessState::Alive;
        PubXmrigApi::update_from_output(&public, &output_parse, &output_pub, elapsed, &process);
        println!("{:#?}", process);
        assert!(process.lock().unwrap().state == ProcessState::NotMining);

        let output_parse = Arc::new(Mutex::new(String::from("[2022-02-12 12:49:30.311]  net      new job from 192.168.2.1:3333 diff 402K algo rx/0 height 2241142 (11 tx)")));
        PubXmrigApi::update_from_output(&public, &output_parse, &output_pub, elapsed, &process);
        assert!(process.lock().unwrap().state == ProcessState::Alive);
    }

    #[test]
    fn serde_priv_p2pool_local_api() {
        let data = r#"{
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
        let priv_api = PrivP2poolLocalApi::from_str(data).unwrap();
        let json = serde_json::ser::to_string_pretty(&priv_api).unwrap();
        println!("{}", json);
        let data_after_ser = r#"{
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
    fn serde_priv_p2pool_network_api() {
        let data = r#"{
				"difficulty": 319028180924,
				"hash": "22ae1b83d727bb2ff4efc17b485bc47bc8bf5e29a7b3af65baf42213ac70a39b",
				"height": 2776576,
				"reward": 600499860000,
				"timestamp": 1670953659
			}"#;
        let priv_api = PrivP2poolNetworkApi::from_str(data).unwrap();
        let json = serde_json::ser::to_string_pretty(&priv_api).unwrap();
        println!("{}", json);
        let data_after_ser = r#"{
  "difficulty": 319028180924,
  "hash": "22ae1b83d727bb2ff4efc17b485bc47bc8bf5e29a7b3af65baf42213ac70a39b",
  "height": 2776576,
  "reward": 600499860000,
  "timestamp": 1670953659
}"#;
        assert_eq!(data_after_ser, json)
    }

    #[test]
    fn serde_priv_p2pool_pool_api() {
        let data = r#"{
				"pool_list": ["pplns"],
				"pool_statistics": {
					"hashRate": 10225772,
					"miners": 713,
					"totalHashes": 487463929193948,
					"lastBlockFoundTime": 1670453228,
					"lastBlockFound": 2756570,
					"totalBlocksFound": 4
				}
			}"#;
        let priv_api = crate::helper::p2pool::PrivP2poolPoolApi::from_str(data).unwrap();
        let json = serde_json::ser::to_string_pretty(&priv_api).unwrap();
        println!("{}", json);
        let data_after_ser = r#"{
  "pool_statistics": {
    "hashRate": 10225772,
    "miners": 713
  }
}"#;
        assert_eq!(data_after_ser, json)
    }

    #[test]
    fn serde_priv_xmrig_api() {
        let data = r#"{
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
        use crate::helper::xmrig::PrivXmrigApi;
        let priv_api = serde_json::from_str::<PrivXmrigApi>(data).unwrap();
        let json = serde_json::ser::to_string_pretty(&priv_api).unwrap();
        println!("{}", json);
        let data_after_ser = r#"{
  "worker_id": "hinto",
  "resources": {
    "load_average": [
      10.97,
      10.58,
      10.47
    ]
  },
  "connection": {
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

    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
        thread,
    };

    use crate::{
        disk::state::P2pool,
        helper::{p2pool::PubP2poolApi, xmrig::PubXmrigApi, xvb::rounds::XvbRound},
        macros::lock,
        XVB_TIME_ALGO,
    };

    use crate::helper::xvb::{public_stats::XvbPubStats, PubXvbApi};
    use reqwest::Client;

    #[test]
    fn public_api_deserialize() {
        let client = Client::new();
        let new_data = thread::spawn(move || corr(&client)).join().unwrap();
        assert!(!new_data.reward_yearly.is_empty());
    }
    #[tokio::main]
    async fn corr(client: &Client) -> XvbPubStats {
        XvbPubStats::request_api(client).await.unwrap()
    }

    #[test]
    fn algorithm_time_given() {
        let gui_api_xvb = Arc::new(Mutex::new(PubXvbApi::new()));
        let gui_api_p2pool = Arc::new(Mutex::new(PubP2poolApi::new()));
        let gui_api_xmrig = Arc::new(Mutex::new(PubXmrigApi::new()));
        let state_p2pool = P2pool::default();
        lock!(gui_api_p2pool).p2pool_difficulty_u64 = 95000000;
        let share = 1;
        // verify that if one share found (enough for vip round) but not enough for donor round, no time will be given to xvb, except if in hero mode.
        // 15mn average HR of xmrig is 5kH/s
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = 0.0;
        lock!(gui_api_xmrig).hashrate_raw_15m = 5000.0;
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::Auto;
        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        // verify that default mode will give x seconds
        assert_eq!(given_time, 0);
        // given time should always be less than XVB_TIME_ALGO
        assert!(given_time < XVB_TIME_ALGO);
        // verify that right round should be detected.
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        lock!(gui_api_xvb).stats_priv.donor_24hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        assert_eq!(round_type(share, &gui_api_xvb), Some(XvbRound::Vip));
        // verify that hero mode will give x seconds
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::Hero;
        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        assert_eq!(given_time, 45);
        // verify that right round should be detected.
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        lock!(gui_api_xvb).stats_priv.donor_24hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        assert_eq!(round_type(share, &gui_api_xvb), Some(XvbRound::Vip));
        // verify that if one share and not enough for donor vip round (should be in donor round), right amount of time will be given to xvb for default and hero mode
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = 0.0;
        lock!(gui_api_xmrig).hashrate_raw_15m = 8000.0;
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::Auto;
        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        // verify that default mode will give x seconds
        assert_eq!(given_time, 75);
        // given time should always be less than XVB_TIME_ALGO
        assert!(given_time < XVB_TIME_ALGO);
        // verify that right round should be detected.
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        lock!(gui_api_xvb).stats_priv.donor_24hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        assert_eq!(round_type(share, &gui_api_xvb), Some(XvbRound::Donor));
        // verify that hero mode will give x seconds
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::Hero;
        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        assert_eq!(given_time, 253);
        // verify that right round should be detected.
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        lock!(gui_api_xvb).stats_priv.donor_24hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        assert_eq!(round_type(share, &gui_api_xvb), Some(XvbRound::Donor));
        // verify that if one share and not enough for donor whale round(should be in donor vip), right amount of time will be given to xvb for default and hero mode
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = 0.0;
        lock!(gui_api_xmrig).hashrate_raw_15m = 19000.0;
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::Auto;
        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        // verify that default mode will give x seconds
        assert_eq!(given_time, 316);
        // given time should always be less than XVB_TIME_ALGO
        assert!(given_time < XVB_TIME_ALGO);
        // verify that right round should be detected.
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        lock!(gui_api_xvb).stats_priv.donor_24hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        assert_eq!(round_type(share, &gui_api_xvb), Some(XvbRound::DonorVip));
        // verify that hero mode will give x seconds
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::Hero;
        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        assert_eq!(given_time, 454);
        // verify that right round should be detected.
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        lock!(gui_api_xvb).stats_priv.donor_24hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        assert_eq!(round_type(share, &gui_api_xvb), Some(XvbRound::DonorVip));
        // verify that if one share and not enough for donor mega round, right amount of time will be given to xvb for default and hero mode
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = 0.0;
        lock!(gui_api_xmrig).hashrate_raw_15m = 105000.0;
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::Auto;
        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        // verify that default mode will give x seconds
        assert_eq!(given_time, 572);
        // given time should always be less than XVB_TIME_ALGO
        assert!(given_time < XVB_TIME_ALGO);
        // verify that right round should be detected.
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        lock!(gui_api_xvb).stats_priv.donor_24hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        assert_eq!(round_type(share, &gui_api_xvb), Some(XvbRound::DonorWhale));
        // verify that hero mode will give x seconds
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::Hero;
        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        assert_eq!(given_time, 573);
        // verify that right round should be detected.
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        lock!(gui_api_xvb).stats_priv.donor_24hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        assert_eq!(round_type(share, &gui_api_xvb), Some(XvbRound::DonorWhale));
        // verify that if one share and enough for donor mega round, right amount of time will be given to xvb for default and hero mode
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = 0.0;
        lock!(gui_api_xmrig).hashrate_raw_15m = 1205000.0;
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::Auto;
        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        // verify that default mode will give x seconds
        assert_eq!(given_time, 498);
        // given time should always be less than XVB_TIME_ALGO
        assert!(given_time < XVB_TIME_ALGO);
        // verify that right round should be detected.
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        lock!(gui_api_xvb).stats_priv.donor_24hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        assert_eq!(round_type(share, &gui_api_xvb), Some(XvbRound::DonorMega));
        // verify that hero mode will give x seconds
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::Hero;
        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        assert_eq!(given_time, 597);
        // verify that right round should be detected.
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        lock!(gui_api_xvb).stats_priv.donor_24hr_avg = ((given_time as f32 / XVB_TIME_ALGO as f32)
            * lock!(gui_api_xmrig).hashrate_raw_15m)
            / 1000.0;
        assert_eq!(round_type(share, &gui_api_xvb), Some(XvbRound::DonorMega));
        // verify that if one share and enough for donor vp round if XvB oHR is given, right amount of time will be given to xvb for default and hero mode
        lock!(gui_api_xvb).output.clear();
        lock!(gui_api_xmrig).hashrate_raw_15m = 12500.0;
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = 5.0;
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::Auto;
        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        // verify that default mode will give x seconds
        assert_eq!(given_time, 240);
        // given time should always be less than XVB_TIME_ALGO
        assert!(given_time < XVB_TIME_ALGO);
        // verify that right round should be detected.
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg =
            (((given_time as f32 / XVB_TIME_ALGO as f32) * lock!(gui_api_xmrig).hashrate_raw_15m)
                + 5000.0)
                / 1000.0;
        lock!(gui_api_xvb).stats_priv.donor_24hr_avg =
            (((given_time as f32 / XVB_TIME_ALGO as f32) * lock!(gui_api_xmrig).hashrate_raw_15m)
                + 5000.0)
                / 1000.0;
        assert_eq!(round_type(share, &gui_api_xvb), Some(XvbRound::DonorVip));
        // verify that hero mode will give x seconds
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = 5.0;
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::Hero;
        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        assert_eq!(given_time, 378);
        // verify that right round should be detected.
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg =
            (((given_time as f32 / XVB_TIME_ALGO as f32) * lock!(gui_api_xmrig).hashrate_raw_15m)
                + 5000.0)
                / 1000.0;
        lock!(gui_api_xvb).stats_priv.donor_24hr_avg =
            (((given_time as f32 / XVB_TIME_ALGO as f32) * lock!(gui_api_xmrig).hashrate_raw_15m)
                + 5000.0)
                / 1000.0;
        assert_eq!(round_type(share, &gui_api_xvb), Some(XvbRound::DonorVip));
    }

    #[test]
    fn test_xvb_advanced_options() {
        let gui_api_xvb = Arc::new(Mutex::new(PubXvbApi::new()));
        let gui_api_p2pool = Arc::new(Mutex::new(PubP2poolApi::new()));
        let gui_api_xmrig = Arc::new(Mutex::new(PubXmrigApi::new()));
        let state_p2pool = P2pool::default();
        lock!(gui_api_p2pool).p2pool_difficulty_u64 = 95000000;
        lock!(gui_api_xvb).stats_priv.donor_1hr_avg = 0.0;
        lock!(gui_api_xmrig).hashrate_raw_15m = 5000.0;
        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::ManualXvb;
        lock!(gui_api_xvb).stats_priv.runtime_manual_amount = 500.0;

        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );

        assert_eq!(given_time, 60);

        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::ManualP2pool;

        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        assert_eq!(given_time, 540);

        lock!(gui_api_xvb).stats_priv.runtime_mode = RuntimeMode::ManualDonationLevel;
        lock!(gui_api_xvb).stats_priv.runtime_manual_donation_level = RuntimeDonationLevel::Donor;

        let given_time = calcul_donated_time(
            lock!(gui_api_xmrig).hashrate_raw_15m,
            &gui_api_p2pool,
            &gui_api_xvb,
            &state_p2pool,
        );
        assert_eq!(given_time, 120);
    }
}
