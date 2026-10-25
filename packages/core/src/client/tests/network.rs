//! What differs between Lighter and Lighter on Robinhood Chain, from each host's own answers.

use super::*;

#[test]
fn each_network_signs_for_its_own_chain_and_settles_on_its_own_l1() {
    assert_eq!((Network::Lighter.chain_id(), Network::Robinhood.chain_id()), (304, 466_324));
    assert_eq!((Network::Lighter.l1_chain_id(), Network::Robinhood.l1_chain_id()), (1, 4663));
    assert_eq!(Network::Lighter.contract().to_hex(), "0x3b4d794a66304f130a4db8f2551b0070dfcf5ca7");
    assert_eq!(Network::Robinhood.contract().to_hex(), "0x94bab9693ba2f6358507effcbd372b0660afff9d");
    assert_eq!(Network::Lighter.collateral().token.to_hex(), "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48");
    assert_eq!(Network::Robinhood.collateral().token.to_hex(), "0x5fc5360d0400a0fd4f2af552add042d716f1d168");
    for network in Network::ALL {
        let collateral = network.collateral();
        assert_eq!((collateral.asset.get(), collateral.decimals, collateral.l1_decimals), (3, 6, 6), "{network}");
        assert!(network.ws_readonly_url().starts_with(network.ws_url()));
        assert!(network.api_url().starts_with("https://") && !network.api_url().ends_with('/'));
    }
    assert_eq!((USDC.symbol, USDG.symbol), ("USDC", "USDG"));
}

#[test]
fn the_venue_s_own_apps_hold_their_slots() {
    let slot = |index| crate::utils::ApiKeyIndex::new(index).unwrap();
    for (index, lighter, robinhood) in [(0, true, true), (3, true, true), (4, false, false), (157, false, true)] {
        assert_eq!(Network::Lighter.reserves_api_key(slot(index)), lighter, "{index}");
        assert_eq!(Network::Robinhood.reserves_api_key(slot(index)), robinhood, "{index}");
    }
    assert!(Network::Lighter.has_binary_options() && !Network::Robinhood.has_binary_options());
}

#[test]
fn a_network_reads_from_its_names() {
    for (text, network) in [
        ("lighter", Network::Lighter),
        (" Mainnet ", Network::Lighter),
        ("zklighter", Network::Lighter),
        ("robinhood", Network::Robinhood),
        ("RH", Network::Robinhood),
        ("lighter-rh", Network::Robinhood),
    ] {
        assert_eq!(text.parse::<Network>(), Ok(network), "{text}");
    }
    assert_eq!("testnet".parse::<Network>(), Err(UnknownNetwork("testnet".to_owned())));
    assert_eq!(Network::Robinhood.to_string(), "robinhood");
    assert_eq!(Network::default(), Network::Lighter);
}
