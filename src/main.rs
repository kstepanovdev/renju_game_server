use core::{panic, time};
use std::collections::HashMap;
use std::io::{stdin, Read, Write};
use std::net::{IpAddr, };
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread::sleep;

use serde::{Deserialize, Serialize};

use tokio::net::{TcpListener, TcpStream};

mod game;
use game::{ServerResponse, Game};
use tokio::sync::oneshot;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    println!("Enter desired IP or leave it blank to keep a default value:");
    let mut buffer = String::new();
    let address = match stdin().read_line(&mut buffer) {
        Ok(_bytes) => {
            if buffer.trim().is_empty() {
                "0.0.0.0:3333"
            } else {
                buffer.trim()
            }
        }
        Err(e) => {
            panic!("{}", e)
        }
    };
    let listener = TcpListener::bind(address).await.unwrap();
    println!("Server listening on ip:port = {}", address);

    let game = Arc::new(RwLock::new(Game::new()));
    let mut clients: HashMap<IpAddr, Receiver<ServerResponse>> = HashMap::new();
    let (server_tx, server_rx): (Sender<ServerResponse>, Receiver<ServerResponse>) = channel();
        loop {
            if let Ok((socket, addr)) = listener.accept().await {
                println!("Client {:?} connected", socket.peer_addr());
                let (client_tx, client_rx) = oneshot::channel();
                clients.insert(addr.ip(), client_rx);

                let game = Arc::clone(&game);
                let tx = server_tx.clone();

                tokio::spawn(async move {
                    process(socket, game, tx).await
                });
        };

        match server_rx.try_recv() {
            Ok(response) => match response {
                ServerResponse::Move { move_id, color, winner } => {
                    let resp = bincode::serialize(&ServerResponse::Move {
                        move_id,
                        color,
                        winner
                    }).unwrap();
                    for (client,  mut socket) in clients {
                        tracing::error!("{:?}", resp);
                        socket.write_all(&resp);
                    }
                }
                ServerResponse::Reset => {
                    let resp = bincode::serialize(&ServerResponse::Reset).unwrap();
                    for (addr, mut socket) in clients {
                        tracing::error!("{:?}", resp);
                        socket.write_all(&resp);
                    }
                }
                ServerResponse::Ok { player_ip } => {
                    let resp = bincode::serialize(&ServerResponse::Ok { player_ip }).unwrap();
                    clients
                        .get_mut(&player_ip)
                        .unwrap()
                        .write_all(&resp);
                }
                ServerResponse::Fail { message, player_ip } => {
                    let resp =
                        bincode::serialize(&ServerResponse::Fail { message, player_ip }).unwrap();
                    clients
                        .get_mut(&player_ip)
                        .unwrap()
                        .write_all(&resp);
                }
            },
            Err(e) => {
                tracing::error!("Failed to receive a value from the rx: {}", e);
            }
        }
        sleep(time::Duration::from_millis(100));
    }
}

async fn process(mut socket: TcpStream, game: Arc<RwLock<Game>>, tx: Sender<ServerResponse>) {
    let mut data = [0; 64];
    let player_ip = socket.peer_addr().unwrap().ip();
    socket.readable().await;
    match socket.try_read(&mut data) {
        Ok(size) => {
            if size == 0 {
                return;
            }

            let response = game.write().unwrap().handle_action(&data, player_ip);
            tracing::error!("{:?}", response);
            if let Err(e) = tx.send(response) {
                tracing::error!("Sending message to a transmitter failed: {}", e)
            };
        }
        Err(e) => {
            println!("Data read error: {}", e);
        }
    }
}