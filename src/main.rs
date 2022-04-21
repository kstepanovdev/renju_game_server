use core::panic;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, self, Sender};
use std::thread;
use std::net::{TcpListener, TcpStream, Shutdown};
use std::io::{Read, Write, stdin};

use serde::{Serialize, Deserialize};

struct Player {
    name: String,
    color: Option<usize>
}

struct Field {
    field: [usize; 225],
    winner: Option<Player>,
}

impl Player {
    fn new(name: String) -> Self {
        Player {
            name,
            color: None
        }
    }
}

#[derive(Serialize, Deserialize)]
enum GameAction {
    Connect(String),
    Move(usize),
}

struct Game {
    players: Vec<Player>,
    active_player: Option<Player>,
    winner: Option<Player>
}

impl Game {
    fn new() -> Self {
        Game { 
            players: vec![],
            active_player: None,
            winner: None,
        }
    }
}

fn handle_client(mut stream: TcpStream, game: Arc<Mutex<Game>>) {
    let mut data = [0; 1024];
    match stream.read(&mut data) {
        Ok(size) => {
            handle_game_action(&data, &stream, game);
        },
        Err(_) => {
            println!("An error occurred, terminating connection with {}", stream.peer_addr().unwrap());
            stream.shutdown(Shutdown::Both).unwrap();
        }
    }
}

fn handle_game_action(data: &[u8], mut stream: &TcpStream, game: Arc<Mutex<Game>>) {
    match bincode::deserialize(data) {
        Ok(GameAction::Connect(name)) => {
            let resp = bincode::serialize(&name).unwrap();
            let new_player = Player::new(name);
            if game.lock().unwrap().players.len() < 2 {
                game.lock().unwrap().players.push(new_player); 
            }
        }
        Ok(GameAction::Move(idx)) => {
            let move_idx = idx;

        },
        Err(e) => { panic!("{}", e) },
    }
}

fn main() {
    println!("Enter desired IP or leave it blank to keep a default value:");
    let mut buffer = String::new();
    let address = match stdin().read_line(&mut buffer) {
        Ok(_bytes) => { if buffer.trim().is_empty() { "0.0.0.0:3333" } else { buffer.trim() } },
        Err(e) => { panic!("{}", e) }
    };
    
    let game = Arc::new(Mutex::new(Game::new()));
    println!("Server listening on ip:port = {}", address);
    loop {
        let listener = TcpListener::bind(address).unwrap();
        match listener.accept() {
            Ok((stream, _addr)) => {
                let game_state = Arc::clone(&game);
                thread::spawn(move || {
                    handle_client(stream, game_state);
                });
            },
            Err(e) => {
                drop(listener);
                println!("couldn't get client: {:?}", e);
            }
        }
    }
}
