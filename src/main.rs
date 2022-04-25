use core::panic;
use std::io::{stdin, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, channel, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread;

use bincode::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
struct Player {
    name: String,
    color: Option<usize>,
    stream: TcpStream,
}

impl Player {
    fn new(name: String, stream: TcpStream) -> Self {
        Player {
            name,
            color: None,
            stream,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
enum GameAction {
    Connect(String),
    Move(usize, String),
    Reset,
}

#[derive(Serialize, Deserialize, Debug)]
enum ServerResponse {
    Ok,
    Fail,
    Move(usize, usize),
}

struct Game {
    players: Vec<Player>,
    active_player: Option<usize>,
    winner: Option<usize>,
    field: [usize; 255],
}

impl Game {
    fn new() -> Self {
        Game {
            players: vec![],
            active_player: None,
            winner: None,
            field: [0; 255],
        }
    }

    fn reset(&mut self) {
        self.active_player = None;
        self.winner = None;
        self.field = [0; 255];
    }

    fn winner_check(&mut self, winner_id: usize, winner_color: usize) {
        self.horizontal_check(winner_id, winner_color);
        [14, 15, 16].map(|shift| self.shift_check(winner_id, winner_color, shift));
    }

    fn horizontal_check(&mut self, winner_id: usize, winner_color: usize) {
        let rows = self.field.chunks(15);
        for row in rows {
            let mut win_line = vec![];
            let mut idx = 0;
            while idx < row.len() {
                let cell_color = row[idx];
                if cell_color == winner_color {
                    win_line.push(idx);
                } else {
                    win_line = vec![];
                }
                if win_line.len() >= 5 {
                    self.winner = Some(winner_id);
                    return;
                }
                idx += 1;
            }
        }
    }

    fn shift_check(&mut self, winner_id: usize, winner_color: usize, shift: usize) {
        let mut idx = 0;
        let mut win_line = vec![];
        while idx < self.field.len() {
            if self.field[idx] != winner_color {
                idx += 1;
                win_line = vec![];
                continue;
            }
            win_line.push(idx);
            let mut i = idx;
            while i + shift < self.field.len() && self.field[i + shift] == winner_color {
                win_line.push(i);
                if win_line.len() >= 5 {
                    self.winner = Some(winner_id);
                    return;
                }
                i += shift;
            }
            win_line = vec![];
            idx += 1;
        }
    }
}

fn handle_client(mut stream: TcpStream, game: Arc<RwLock<Game>>, tx: Sender<ServerResponse>) {
    let mut data = [0; 1024];
    match stream.read(&mut data) {
        Ok(size) => {
            handle_game_action(&data, stream, game, tx);
        }
        Err(_) => {
            println!(
                "An error occurred, terminating connection with {}",
                stream.peer_addr().unwrap()
            );
            stream.shutdown(Shutdown::Both).unwrap();
        }
    }
}

fn handle_game_action(
    data: &[u8],
    stream: TcpStream,
    game: Arc<RwLock<Game>>,
    tx: Sender<ServerResponse>,
) {
    let data = bincode::deserialize::<GameAction>(data);
    match data {
        Ok(GameAction::Reset) => {
            game.write().unwrap().reset();
            tx.send(ServerResponse::Ok).unwrap();
        }

        Ok(GameAction::Connect(name)) => {
            let new_player = Player::new(name, stream);
            let mut game = game.write().unwrap();
            game.players.push(new_player);
            tracing::error!("{:?}", &game.players);
            tx.send(ServerResponse::Ok).unwrap();
        }

        Ok(GameAction::Move(move_id, name)) => {
            let mut state = game.write().unwrap();
            let (player_id, second_player_id) = if state.players[0].name == name {
                (0_usize, 1_usize)
            } else {
                (1_usize, 0_usize)
            };
            if player_id != state.active_player.unwrap() {
                return;
            }
            match state.active_player {
                Some(active_player_id) => {
                    if active_player_id == 0_usize {
                        state.active_player = Some(1)
                    } else {
                        state.active_player = Some(0)
                    };
                }
                None => {
                    state.active_player = Some(second_player_id);
                    state.players[player_id].color = Some(1_usize);
                    state.players[second_player_id].color = Some(0_usize);
                }
            }
            let player_color = state.players[player_id].color.unwrap();
            state.field[move_id] = player_id;
            state.winner_check(player_id, player_color);
            tx.send(ServerResponse::Move(move_id, player_color))
                .unwrap();
        }
        Err(e) => {
            panic!("{}", e)
        }
    }
}

fn main() {
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

    let game = Arc::new(RwLock::new(Game::new()));
    println!("Server listening on ip:port = {}", address);
    let listener = TcpListener::bind(address).unwrap();
    let (tx, rx): (Sender<ServerResponse>, Receiver<ServerResponse>) = channel();
    let mut clients = vec![];

    for stream in listener.incoming() {
        println!(
            "Client {:?} connected",
            stream.as_ref().unwrap().peer_addr()
        );
        let stream = stream.unwrap();
        clients.push(stream.try_clone().unwrap());
        let tx_copy = tx.clone();
        let game_state = Arc::clone(&game);
        thread::spawn(move || {
            handle_client(stream, game_state, tx_copy);
        });

        match rx.recv() {
            Ok(response) => match response {
                ServerResponse::Move(move_id, player_color) => {
                    let resp =
                        bincode::serialize(&ServerResponse::Move(move_id, player_color)).unwrap();
                    tracing::error!("MOVEMOVEMOVEMOVEMOVE");
                    for mut client in &clients {
                        client.write_all(&resp).unwrap();
                    }
                }
                ServerResponse::Ok => {
                    tracing::error!("OKOKOKOK");
                    for mut client in &clients {
                        let resp = bincode::serialize(&ServerResponse::Ok).unwrap();
                        client.write_all(&resp).unwrap();
                    }
                }
                _ => {
                    tracing::error!("?????????");
                }
            },
            Err(e) => {
                tracing::error!("Failed to receive a value from the rx: {}", e);
            }
        }
    }
}
