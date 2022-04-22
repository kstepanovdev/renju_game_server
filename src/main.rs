use core::panic;
use std::collections::HashMap;
use std::io::{stdin, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone)]
struct Player {
    name: String,
    color: Option<usize>,
}

impl Player {
    fn new(name: String) -> Self {
        Player { name, color: None }
    }
}

#[derive(Serialize, Deserialize)]
enum GameAction {
    Connect(String),
    Move(usize, String),
}

struct Game {
    players: Option<Vec<Player>>,
    active_player: Option<usize>,
    winner: Option<usize>,
    field: [usize; 255],
}

impl Game {
    fn new() -> Self {
        Game {
            players: None,
            active_player: None,
            winner: None,
            field: [0; 255],
        }
    }

    fn winner_check(&mut self, winner_id: usize) {
        self.horizontal_check(winner_id);
        [14, 15, 16].map(|shift| self.shift_check(winner_id, shift));
    }

    fn horizontal_check(&mut self, winner_id: usize) {
        let rows = self.field.chunks(15);
        for row in rows {
            let mut win_line = vec![];
            let mut idx = 0;
            while idx < row.len() {
                let cell_color = row[idx];
                if cell_color == winner_id {
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

    fn shift_check(&mut self, winner_id: usize, shift: usize) {
        let mut idx = 0;
        let mut win_line = vec![];
        while idx < self.field.len() {
            if self.field[idx] != winner_id {
                idx += 1;
                win_line = vec![];
                continue;
            }
            win_line.push(idx);
            let mut i = idx;
            while i + shift < self.field.len() && self.field[i + shift] == winner_id {
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

fn handle_client(mut stream: TcpStream, game: Arc<Mutex<Game>>, rx: Sender<GameAction>) {
    let mut data = [0; 1024];
    match stream.read(&mut data) {
        Ok(size) => {
            handle_game_action(&data, &stream, game);
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

fn handle_game_action(data: &[u8], mut stream: &TcpStream, game: Arc<Mutex<Game>>) {
    match bincode::deserialize(data) {
        Ok(GameAction::Connect(name)) => {
            let new_player = Player::new(name);
            let mut state = game.lock().unwrap();
            match &mut state.players {
                Some(players) => {
                    players.push(new_player);
                }
                None => {
                    state.players = Some(vec![new_player]);
                }
            }
        }

        Ok(GameAction::Move(move_id, name)) => {
            let mut state = game.lock().unwrap();
            let player_id = if state.players.as_ref().unwrap()[0].name == name {
                0_usize
            } else {
                1_usize
            };
            if player_id != state.active_player.unwrap() {
                return;
            }
            match &mut state.active_player {
                Some(active_player_id) => {
                    if active_player_id == &mut 0_usize {
                        *active_player_id = 1
                    } else {
                        *active_player_id = 0
                    };
                }
                None => {
                    state.active_player = Some(player_id);
                }
            }
            state.field[move_id] = player_id;
            game.lock().unwrap().winner_check(player_id);
        }
        Err(e) => {
            panic!("{}", e)
        }
    }
}

fn main() {
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

    let game = Arc::new(Mutex::new(Game::new()));
    println!("Server listening on ip:port = {}", address);
    let (tx, rx) = channel();
    loop {
        let listener = TcpListener::bind(address).unwrap();
        match listener.accept() {
            Ok((stream, _addr)) => {
                let tx_copy = tx.clone();
                let game_state = Arc::clone(&game);
                thread::spawn(move || {
                    handle_client(stream, game_state, tx_copy);
                });
            }
            Err(e) => {
                drop(listener);
                println!("couldn't get client: {:?}", e);
            }
        }
    }
}
