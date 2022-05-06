use core::panic;
use std::collections::HashMap;
use std::io::{self, stdin, Read, Write};
use std::net::IpAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use std::error::Error;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;
use tokio_util::codec::{BytesCodec, Framed};

mod game;
use game::{Game, ServerResponse};

type Tx = mpsc::UnboundedSender<String>;
type Rx = mpsc::UnboundedReceiver<String>;

struct Shared {
    peers: HashMap<SocketAddr, Tx>,
}
impl Shared {
    /// Create a new, empty, instance of `Shared`.
    fn new() -> Self {
        Shared {
            peers: HashMap::new(),
        }
    }

    /// Send a `LineCodec` encoded message to every peer, except
    /// for the sender.
    async fn broadcast(&mut self, sender: SocketAddr, message: &str) {
        for peer in self.peers.iter_mut() {
            if *peer.0 != sender {
                let _ = peer.1.send(message.into());
            }
        }
    }
}

struct Peer {
    lines: Framed<TcpStream, BytesCodec>,
    rx: Rx,
}

impl Peer {
    async fn new(
        state: Arc<Mutex<Shared>>,
        lines: Framed<TcpStream, BytesCodec>,
    ) -> io::Result<Peer> {
        let addr = lines.get_ref().peer_addr()?;
        let (tx, rx) = mpsc::unbounded_channel();
        state.lock().await.peers.insert(addr, tx);

        Ok(Peer { lines, rx })
    }
}

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
    let state = Arc::new(Mutex::new(Shared::new()));
    let game = Arc::new(Mutex::new(Game::new()));

    loop {
        let (stream, addr) = listener.accept().await.unwrap();
        let state = Arc::clone(&state);
        let game = Arc::clone(&game);

        tokio::spawn(async move {
            if let Err(e) = process(state, stream, addr, game).await {
                tracing::info!("an error occurred; error = {:?}", e);
            }
        });
    }
}

async fn process(
    state: Arc<Mutex<Shared>>,
    stream: TcpStream,
    addr: SocketAddr,
    game: Arc<Mutex<Game>>,
) -> Result<(), Box<dyn Error>> {
    let player_ip = stream.peer_addr().unwrap().ip();
    let mut lines = Framed::new(stream, BytesCodec::new());
    let mut peer = Peer::new(state.clone(), lines).await?;

    loop {
        tokio::select! {
            // A message was received from a peer. Send it to the current user.
            // Some(msg) = peer.rx.recv() => {
                // peer.lines.send(&msg).await?;
            // }
            result = peer.lines.next() => match result {
                Some(Ok(data)) => {
                    // let mut state = state.lock().await;
                    // let msg = format!("{}: {}", username, msg);
                    // state.broadcast(addr, &msg).await;
                    let response = game.lock().await.handle_action(&data, player_ip);
                }
                // An error occurred.
                Some(Err(e)) => {
                    tracing::error!(
                        "an error occurred while processing messages for {}; error = {:?}",
                        "kek",
                        e
                    );
                }
                // The stream has been exhausted.
                None => break,
            },
        }
    }

    // If this section is reached it means that the client was disconnected!
    // Let's let everyone still connected know about it.
    //     {
    //         let mut state = state.lock().await;
    //         state.peers.remove(&addr);

    //         let msg = format!("{} has left the chat", username);
    //         tracing::info!("{}", msg);
    //         state.broadcast(addr, &msg).await;
    //     }

    Ok(())
}

// // async fn process(socket: OwnedReadHalf, game: Arc<RwLock<Game>>, tx: Sender<ServerResponse>) {
// //     let mut data = [0; 64];
// //     let player_ip = socket.peer_addr().unwrap().ip();
// //     socket.readable().await;
// //     match socket.try_read(&mut data) {
// //         Ok(size) => {
// //             if size == 0 {
// //                 return;
//             }

//             let response = game.write().unwrap().handle_action(&data, player_ip);
//             tracing::error!("{:?}", response);
//             if let Err(e) = tx.send(response) {
//                 tracing::error!("Sending message to a transmitter failed: {}", e)
//             };
//         }
//         Err(e) => {
//             println!("Data read error: {}", e);
//         }
//     }
// }

// tokio::spawn(async move {
//     // loop {
//         match server_rx.try_recv() {
//             Ok(response) => match response {
//                 ServerResponse::Move {
//                     move_id,
//                     color,
//                     winner,
//                 } => {
//                     let resp = bincode::serialize(&ServerResponse::Move {
//                         move_id,
//                         color,
//                         winner,
//                     })
//                         .unwrap();
//                     for (client, mut socket) in clients {
//                         tracing::error!("{:?}", resp);
//                         socket.write_all(&resp);
//                     }
//                 }
//                 ServerResponse::Reset => {
//                     let resp = bincode::serialize(&ServerResponse::Reset).unwrap();
//                     for (addr, mut socket) in clients {
//                         tracing::error!("{:?}", resp);
//                         socket.write_all(&resp);
//                     }
//                 }
//                 ServerResponse::Ok { player_ip } => {
//                     let resp = bincode::serialize(&ServerResponse::Ok { player_ip }).unwrap();
//                     clients.get_mut(&player_ip).unwrap().write_all(&resp);
//                 }
//                 ServerResponse::Fail { message, player_ip } => {
//                     let resp =
//                         bincode::serialize(&ServerResponse::Fail { message, player_ip }).unwrap();
//                     clients.get_mut(&player_ip).unwrap().write_all(&resp);
//                 }
//             },
//             Err(e) => {
//                 tracing::error!("Failed to receive a value from the rx: {}", e);
//             }
//         }
//     // }
// });
