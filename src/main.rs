use core::panic;
use std::io::{stdin, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use threadpool::ThreadPool;

#[derive(Debug)]
struct Player {
    name: String,
    color: Option<usize>,
}

impl Player {
    fn new(name: String) -> Self {
        Player { name, color: None }
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

fn handle_game_action(data: &[u8], game: Arc<RwLock<Game>>) -> ServerResponse {
    let data = bincode::deserialize::<GameAction>(data);
    match data {
        Ok(GameAction::Reset) => {
            game.write().unwrap().reset();
            ServerResponse::Ok
        }

        Ok(GameAction::Connect(name)) => {
            let new_player = Player::new(name);
            let mut game = game.write().unwrap();
            game.players.push(new_player);
            ServerResponse::Ok
        }

        Ok(GameAction::Move(move_id, name)) => {
            let player_color = 1;
            ServerResponse::Move(move_id, player_color)

            // let mut state = game.write().unwrap();
            // let (player_id, second_player_id) = if state.players[0].name == name {
            //     (0_usize, 1_usize)
            // } else {
            //     (1_usize, 0_usize)
            // };
            // // TODO: enable this
            // // if player_id != state.active_player.unwrap() {
            //     // return;
            // // }
            // match state.active_player {
            //     Some(active_player_id) => {
            //         if active_player_id == 0_usize {
            //             state.active_player = Some(1)
            //         } else {
            //             state.active_player = Some(0)
            //         };
            //     }
            //     None => {
            //         state.active_player = Some(second_player_id);
            //         state.players[player_id].color = Some(1_usize);
            //         state.players[second_player_id].color = Some(0_usize);
            //     }
            // }
            // let player_color = state.players[player_id].color.unwrap();
            // state.field[move_id] = player_id;
            // state.winner_check(player_id, player_color);
            // ServerResponse::Move(move_id, player_color)
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
    // let address = "0.0.0.0:3000";
    let listener = TcpListener::bind(address).unwrap();
    println!("Server listening on ip:port = {}", address);

    let game = Arc::new(RwLock::new(Game::new()));
    let mut clients = vec![];
    let pool = ThreadPool::new(4);

    let (tx, rx): (Sender<ServerResponse>, Receiver<ServerResponse>) = channel();
    for mut stream in listener.incoming() {
        println!(
            "Client {:?} connected",
            stream.as_ref().unwrap().peer_addr()
        );

        clients.push(stream.as_mut().unwrap().try_clone().unwrap());

        let game = Arc::clone(&game);
        // let mut stream_clone = stream.unwrap().try_clone();
        pool.execute(move || loop {
            let arc_game = Arc::clone(&game);

            let mut data = [0; 32];
            match stream.as_mut().unwrap().read(&mut data) {
                Ok(size) => {
                    tracing::warn!(size);
                    if size == 0 {
                        return;
                    }

                    let response = handle_game_action(&data, arc_game);
                    if let Err(e) = tx.send(response) {
                        tracing::error!("{}", e)
                    };
                }
                Err(e) => {
                    println!("Data read error: {}", e);
                }
            }
        });

        loop {
            match rx.recv() {
                Ok(response) => match response {
                    ServerResponse::Move(move_id, player_color) => {
                        let resp = bincode::serialize(&ServerResponse::Move(move_id, player_color))
                            .unwrap();
                        for mut client in &clients {
                            tracing::error!("{:?}", resp);
                            client.write_all(&resp).unwrap();
                        }
                    }
                    ServerResponse::Ok => {
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
}
