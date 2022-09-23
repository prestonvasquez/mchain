use libp2p::floodsub;
use libp2p::NetworkBehaviour;
use libp2p::Swarm;
use log;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::app;

// KEYS is the private key of the local node.
pub static KEYS: Lazy<libp2p::identity::Keypair> =
    Lazy::new(|| libp2p::identity::Keypair::generate_ed25519());

// PEER_ID is used to identify a client on the network.
pub static PEER_ID: Lazy<libp2p::PeerId> = Lazy::new(|| libp2p::PeerId::from(KEYS.public()));

// We initialize two topics (i.e. "channels") that we will use to broadcast messages to all
// connected peers. This methodology uses the floodsub protocol, which is a simple pub/sub
// protocol that broadcasts messages to all connected peers.

// CHAIN_TOP can be subscribed to in order to send our local blockchain to other nodes.
pub static CHAIN_TOP: Lazy<floodsub::Topic> = Lazy::new(|| floodsub::Topic::new("chains"));

// BLOCK_TOP is usd to broadcast and receive new blocks.
pub static BLOCK_TOP: Lazy<floodsub::Topic> = Lazy::new(|| floodsub::Topic::new("blocks"));

#[derive(Debug, Serialize, Deserialize)]
pub struct ChainResponse {
    pub blocks: Vec<app::Block>,
    pub receiver: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalChainRequest {
    pub from_peer_id: String,
}

pub enum Event {
    Input(String),
    Init,
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "AppBehaviorEvent")]
pub struct AppBehavior {
    pub mdns: libp2p::mdns::Mdns,
    pub floodsub: floodsub::Floodsub,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum AppBehaviorEvent {
    Mdns(libp2p::mdns::MdnsEvent),
    Floodsub(floodsub::FloodsubEvent),
}

impl From<libp2p::mdns::MdnsEvent> for AppBehaviorEvent {
    fn from(event: libp2p::mdns::MdnsEvent) -> Self {
        Self::Mdns(event)
    }
}

impl From<floodsub::FloodsubEvent> for AppBehaviorEvent {
    fn from(event: floodsub::FloodsubEvent) -> Self {
        Self::Floodsub(event)
    }
}

// get_peers returns a list of peers that are currently connected to the swarm.
pub fn get_peers(swarm: &Swarm<AppBehavior>) -> Vec<String> {
    let nodes = swarm.behaviour().mdns.discovered_nodes();
    let mut unique_peers = HashSet::new();
    for peer in nodes {
        unique_peers.insert(peer);
    }
    unique_peers.iter().map(|p| p.to_string()).collect()
}

// print_peers prints a list of peers that are currently connected to the swarm.
pub fn print_peers(swarm: &Swarm<AppBehavior>) {
    get_peers(swarm).iter().for_each(|p| log::info!("{}", p));
}
