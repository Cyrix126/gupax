//---------------------------------------------------------------------------------------------------- TESTS
#[cfg(test)]
mod test {
    use crate::disk::node::Node;
    use crate::disk::pool::Pool;
    use crate::disk::state::State;
    #[test]
    fn serde_default_state() {
        let state = State::new();
        let string = State::to_string(&state).unwrap();
        State::from_str(&string).unwrap();
    }
    #[test]
    fn serde_default_node() {
        let node = Node::new_vec();
        let string = Node::to_string(&node).unwrap();
        Node::from_str_to_vec(&string).unwrap();
    }
    #[test]
    fn serde_default_pool() {
        let pool = Pool::new_vec();
        let string = Pool::to_string(&pool).unwrap();
        Pool::from_str_to_vec(&string).unwrap();
    }

    #[test]
    fn serde_custom_state() {
        let state = r#"
			[gupax]
			simple = true
			auto_update = true
			auto_p2pool = false
			auto_xmrig = false
            auto_xvb = false
			ask_before_quit = true
			save_before_quit = true
			p2pool_path = "p2pool/p2pool"
			xmrig_path = "xmrig/xmrig"
			absolute_p2pool_path = "/home/hinto/p2pool/p2pool"
			absolute_xmrig_path = "/home/hinto/xmrig/xmrig"
			selected_width = 1280
			selected_height = 960
			selected_scale = 0.0
			tab = "About"
			ratio = "Width"
			bundled = false

			[status]
			submenu = "P2pool"
			payout_view = "Oldest"
			monero_enabled = true
			manual_hash = false
			hashrate = 1241.23
			hash_metric = "Hash"

			[p2pool]
			simple = true
			mini = true
			auto_ping = true
			auto_select = true
			backup_host = true
			out_peers = 10
			in_peers = 450
			log_level = 3
			node = "Seth"
			arguments = ""
			address = "44hintoFpuo3ugKfcqJvh5BmrsTRpnTasJmetKC4VXCt6QDtbHVuixdTtsm6Ptp7Y8haXnJ6j8Gj2dra8CKy5ewz7Vi9CYW"
			name = "Local Monero Node"
			ip = "192.168.1.123"
			rpc = "18089"
			zmq = "18083"
			selected_index = 0
			selected_name = "Local Monero Node"
			selected_ip = "192.168.1.123"
			selected_rpc = "18089"
			selected_zmq = "18083"

			[xmrig]
			simple = true
			pause = 0
			simple_rig = ""
			arguments = ""
			tls = false
			keepalive = false
			max_threads = 32
			current_threads = 16
			address = ""
			api_ip = "localhost"
			api_port = "18088"
			name = "linux"
			rig = "Gupaxx"
			ip = "192.168.1.122"
			port = "3333"
			selected_index = 1
			selected_name = "linux"
			selected_rig = "Gupaxx"
			selected_ip = "192.168.1.122"
			selected_port = "3333"
            token = "testtoken"

            [xvb]
            token = ""
            hero = false
            node = "Europe"
			[version]
			gupax = "v1.3.0"
			p2pool = "v2.5"
			xmrig = "v6.18.0"
		"#;
        let state = State::from_str(state).unwrap();
        State::to_string(&state).unwrap();
    }

    #[test]
    fn serde_custom_node() {
        let node = r#"
			['Local Monero Node']
			ip = "localhost"
			rpc = "18081"
			zmq = "18083"

			['asdf-_. ._123']
			ip = "localhost"
			rpc = "11"
			zmq = "1234"

			['aaa     bbb']
			ip = "192.168.2.333"
			rpc = "1"
			zmq = "65535"
		"#;
        let node = Node::from_str_to_vec(node).unwrap();
        Node::to_string(&node).unwrap();
    }

    #[test]
    fn serde_custom_pool() {
        let pool = r#"
			['Local P2Pool']
			rig = "Gupaxx_v1.0.0"
			ip = "localhost"
			port = "3333"

			['aaa xx .. -']
			rig = "Gupaxx"
			ip = "192.168.22.22"
			port = "1"

			['           a']
			rig = "Gupaxx_v1.0.0"
			ip = "127.0.0.1"
			port = "65535"
		"#;
        let pool = Pool::from_str_to_vec(pool).unwrap();
        Pool::to_string(&pool).unwrap();
    }

    // Make sure we keep the user's old values that are still
    // valid but discard the ones that don't exist anymore.
    #[test]
    fn merge_state() {
        let bad_state = r#"
			[gupax]
			SETTING_THAT_DOESNT_EXIST_ANYMORE = 123123
			simple = false
			auto_update = true
			auto_p2pool = false
			auto_xmrig = false
			ask_before_quit = true
			save_before_quit = true
			p2pool_path = "p2pool/p2pool"
			xmrig_path = "xmrig/xmrig"
			absolute_p2pool_path = ""
			absolute_xmrig_path = ""
			selected_width = 0
			selected_height = 0
			tab = "About"
			ratio = "Width"

			[p2pool]
			SETTING_THAT_DOESNT_EXIST_ANYMORE = "String"
			simple = true
			mini = true
			auto_ping = true
			auto_select = true
			out_peers = 10
			in_peers = 450
			log_level = 6
			node = "Seth"
			arguments = ""
			address = "44hintoFpuo3ugKfcqJvh5BmrsTRpnTasJmetKC4VXCt6QDtbHVuixdTtsm6Ptp7Y8haXnJ6j8Gj2dra8CKy5ewz7Vi9CYW"
			name = "Local Monero Node"
			ip = "localhost"
			rpc = "18081"
			zmq = "18083"
			selected_index = 0
			selected_name = "Local Monero Node"
			selected_ip = "localhost"
			selected_rpc = "18081"
			selected_zmq = "18083"

			[xmrig]
			SETTING_THAT_DOESNT_EXIST_ANYMORE = true
			simple = true
			pause = 0
			simple_rig = ""
			arguments = ""
			tls = false
			keepalive = false
			max_threads = 32
			current_threads = 16
			address = ""
			api_ip = "localhost"
			api_port = "18088"
			name = "Local P2Pool"
			rig = "Gupaxx_v1.0.0"
			ip = "localhost"
			port = "3333"
			selected_index = 0
			selected_name = "Local P2Pool"
			selected_rig = "Gupaxx_v1.0.0"
			selected_ip = "localhost"
			selected_port = "3333"

            [xvb]
            token = ""
			[version]
			gupax = "v1.0.0"
			p2pool = "v2.5"
			xmrig = "v6.18.0"
		"#.to_string();
        let merged_state = State::merge(&bad_state).unwrap();
        let merged_state = State::to_string(&merged_state).unwrap();
        println!("{}", merged_state);
        assert!(merged_state.contains("simple = false"));
        assert!(merged_state.contains("in_peers = 450"));
        assert!(merged_state.contains("log_level = 6"));
        assert!(merged_state.contains(r#"node = "Seth""#));
        assert!(!merged_state.contains("SETTING_THAT_DOESNT_EXIST_ANYMORE"));
        assert!(merged_state.contains("44hintoFpuo3ugKfcqJvh5BmrsTRpnTasJmetKC4VXCt6QDtbHVuixdTtsm6Ptp7Y8haXnJ6j8Gj2dra8CKy5ewz7Vi9CYW"));
        assert!(merged_state.contains("backup_host = true"));
    }

    #[test]
    fn create_and_serde_gupax_p2pool_api() {
        use crate::disk::gupax_p2pool_api::GupaxP2poolApi;
        use crate::xmr::AtomicUnit;
        use crate::xmr::PayoutOrd;

        // Get API dir, fill paths.
        let mut api = GupaxP2poolApi::new();
        let mut path = crate::disk::get_gupax_data_path().unwrap();
        path.push(crate::disk::GUPAX_P2POOL_API_DIRECTORY);
        GupaxP2poolApi::fill_paths(&mut api, &path);
        println!("{:#?}", api);

        // Create, write some fake data.
        GupaxP2poolApi::create_all_files(&path).unwrap();
        api.log        = "NOTICE  2022-01-27 01:30:23.1377 P2Pool You received a payout of 0.000000000001 XMR in block 2642816".to_string();
        api.payout_u64 = 1;
        api.xmr = AtomicUnit::from_u64(2);
        let (date, atomic_unit, block) = PayoutOrd::parse_raw_payout_line(&api.log);
        let formatted_log_line = GupaxP2poolApi::format_payout(&date, &atomic_unit, &block);
        GupaxP2poolApi::write_to_all_files(&api, &formatted_log_line).unwrap();
        println!("AFTER WRITE: {:#?}", api);

        // Read
        GupaxP2poolApi::read_all_files_and_update(&mut api).unwrap();
        println!("AFTER READ: {:#?}", api);

        // Assert that the file read mutated the internal struct correctly.
        assert_eq!(api.payout_u64, 1);
        assert_eq!(api.xmr.to_u64(), 2);
        assert!(!api.payout_ord.is_empty());
        assert!(api
            .log
            .contains("2022-01-27 01:30:23.1377 | 0.000000000001 XMR | Block 2,642,816"));
    }

    #[test]
    fn convert_hash() {
        use crate::disk::status::Hash;
        let hash = 1.0;
        assert_eq!(Hash::convert(hash, Hash::Hash, Hash::Hash), 1.0);
        assert_eq!(Hash::convert(hash, Hash::Hash, Hash::Kilo), 0.001);
        assert_eq!(Hash::convert(hash, Hash::Hash, Hash::Mega), 0.000_001);
        assert_eq!(Hash::convert(hash, Hash::Hash, Hash::Giga), 0.000_000_001);
        let hash = 1.0;
        assert_eq!(Hash::convert(hash, Hash::Kilo, Hash::Hash), 1_000.0);
        assert_eq!(Hash::convert(hash, Hash::Kilo, Hash::Kilo), 1.0);
        assert_eq!(Hash::convert(hash, Hash::Kilo, Hash::Mega), 0.001);
        assert_eq!(Hash::convert(hash, Hash::Kilo, Hash::Giga), 0.000_001);
        let hash = 1.0;
        assert_eq!(Hash::convert(hash, Hash::Mega, Hash::Hash), 1_000_000.0);
        assert_eq!(Hash::convert(hash, Hash::Mega, Hash::Kilo), 1_000.0);
        assert_eq!(Hash::convert(hash, Hash::Mega, Hash::Mega), 1.0);
        assert_eq!(Hash::convert(hash, Hash::Mega, Hash::Giga), 0.001);
        let hash = 1.0;
        assert_eq!(Hash::convert(hash, Hash::Giga, Hash::Hash), 1_000_000_000.0);
        assert_eq!(Hash::convert(hash, Hash::Giga, Hash::Kilo), 1_000_000.0);
        assert_eq!(Hash::convert(hash, Hash::Giga, Hash::Mega), 1_000.0);
        assert_eq!(Hash::convert(hash, Hash::Giga, Hash::Giga), 1.0);
    }
}
