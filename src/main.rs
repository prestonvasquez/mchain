use async_std::{io, task};
use futures::{
    prelude::{stream::StreamExt, *},
    select,
};
use libp2p::{
    floodsub::{self, Floodsub, FloodsubEvent},
    identity,
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    swarm::SwarmEvent,
    Multiaddr, NetworkBehaviour, PeerId, Swarm,
};
use mongodb::{
    bson::{doc, Document},
    options::{ClientOptions, ResolverConfig},
    Client,
};
use serde;
use std::error::Error;

mod app;
mod p2p;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    // Create a random PeerId
    println!("Local peer id: {:?}", *p2p::PEER_ID);

    // Set up an encrypted DNS-enabled TCP Transport over the Mplex and Yamux protocols
    let transport = libp2p::development_transport(p2p::KEYS.clone()).await?;

    // Create a Floodsub topic
    let floodsub_topic = floodsub::Topic::new("chat");

    // Create a Swarm to manage peers and events
    let mut swarm = {
        let mdns = task::block_on(Mdns::new(MdnsConfig::default()))?;
        let mut behaviour = p2p::AppBehavior {
            floodsub: Floodsub::new(*p2p::PEER_ID),
            mdns,
        };

        behaviour.floodsub.subscribe(floodsub_topic.clone());
        Swarm::new(transport, behaviour, *p2p::PEER_ID)
    };

    // Reach out to another node if specified
    if let Some(to_dial) = std::env::args().nth(1) {
        let addr: Multiaddr = to_dial.parse()?;
        swarm.dial(addr)?;
        println!("Dialed {:?}", to_dial)
    }

    // Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();

    // Listen on all interfaces and whatever port the OS assigns
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // app is a state machine for the blockchain.
    let mut app = app::App::new();

    // Get an MDB client.
    let client_uri = "mongodb://localhost:27017";

    let options =
        ClientOptions::parse_with_resolver_config(&client_uri, ResolverConfig::cloudflare())
            .await?;

    let client = Client::with_options(options)?;

    // Ping the MDB server.
    client
        .database("admin")
        .run_command(doc! {"ping": 1}, None)
        .await?;
    log::info!("Connected to MongoDB!");

    // Initialize the ledger.
    let db = client.database("app");
    let collection = db.collection::<Document>("ledger");

    loop {
        select! {
            line = stdin.select_next_some() => swarm
                .behaviour_mut()
                .floodsub
                .publish(floodsub_topic.clone(), line.expect("Stdin not to close").as_bytes()),

            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {:?}", address);

                    // Generate the genesis block.
                    app.genesis();
                }

                // User messages constitut data on a block chain.
                SwarmEvent::Behaviour(p2p::AppBehaviorEvent::Floodsub(
                    FloodsubEvent::Message(message)
                )) => {
                    // Get the previous block.
                    let latest_block = app.blocks.last().unwrap();

                    // Create a new block with the message data.
                    let block = app::Block::new(latest_block.hash.clone(), message.data.clone());
                    log::info!("New block: {:?}", block);

                    log::info!("Received message: {:?}", message);
                    collection.insert_one(doc! {"data": "hi"}, None).await?;
                }

                // If a peer joins the network, add it to the floodsub viewer.
                SwarmEvent::Behaviour(p2p::AppBehaviorEvent::Mdns(
                    MdnsEvent::Discovered(list)
                )) => {
                    for (peer, _) in list {
                        swarm
                            .behaviour_mut()
                            .floodsub
                            .add_node_to_partial_view(peer);
                    }
                }

                // If a peer leaves the network, remove it from the floodsub viewer.
                SwarmEvent::Behaviour(p2p::AppBehaviorEvent::Mdns(MdnsEvent::Expired(
                    list
                ))) => {
                    for (peer, _) in list {
                        if !swarm.behaviour_mut().mdns.has_node(&peer) {
                            swarm
                                .behaviour_mut()
                                .floodsub
                                .remove_node_from_partial_view(&peer);
                        }
                    }
                },
                _ => {}
            }
        }
    }
}
