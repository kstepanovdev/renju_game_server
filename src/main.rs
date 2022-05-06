use core::panic;
use std::collections::HashMap;
use std::io::{self, stdin};
use std::sync::Arc;

use futures::SinkExt;
use std::error::Error;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;
use tokio_util::codec::{BytesCodec, Framed};

use bytes::{BufMut, BytesMut};

mod game;
use game::{Game, ServerResponse};

type Tx = mpsc::UnboundedSender<Vec<u8>>;
type Rx = mpsc::UnboundedReceiver<Vec<u8>>;

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

    async fn broadcast(&mut self, response: Vec<u8>) {
        for peer in self.peers.iter_mut() {
            let _ = peer.1.send(response.clone());
        }
    }

    async fn direct_message(&mut self, response: Vec<u8>, player_addr: SocketAddr) {
        self.peers[&player_addr].send(response).unwrap();
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
    let lines = Framed::new(stream, BytesCodec::new());
    let mut peer = Peer::new(state.clone(), lines).await?;

    loop {
        tokio::select! {
            // A message was received from a peer. Send it to the current user.
            Some(data) = peer.rx.recv() => {
                let mut buf = BytesMut::with_capacity(64);
                let data: &[u8] = &data;
                buf.put(data);
                peer.lines.send(buf).await?;
            }
            result = peer.lines.next() => match result {
                Some(Ok(data)) => {
                    let response = game.lock().await.handle_action(&data, addr);
                    handle_response(response, Arc::clone(&state)).await;
                }
                Some(Err(e)) => {
                    tracing::error!(
                        "an error occurred while processing messages for {}; error = {:?}",
                        addr,
                        e
                    );
                }
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

async fn handle_response(response: ServerResponse, state: Arc<Mutex<Shared>>) {
    match response {
        ServerResponse::Move {
            move_id,
            color,
            winner,
        } => {
            let response = bincode::serialize(&ServerResponse::Move {
                move_id,
                color,
                winner,
            })
            .unwrap();

            state.lock().await.broadcast(response).await;
        }
        ServerResponse::Reset => {
            let response = bincode::serialize(&ServerResponse::Reset).unwrap();
            state.lock().await.broadcast(response).await;
        }
        ServerResponse::Ok { player_addr } => {
            let response = bincode::serialize(&ServerResponse::Ok { player_addr }).unwrap();
            state
                .lock()
                .await
                .direct_message(response, player_addr)
                .await;
        }
        ServerResponse::Fail {
            message,
            player_addr,
        } => {
            let response = bincode::serialize(&ServerResponse::Fail {
                message,
                player_addr,
            })
            .unwrap();
            state
                .lock()
                .await
                .direct_message(response, player_addr)
                .await;
        }
    }
}
