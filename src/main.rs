use core::{panic, time};
use std::collections::HashMap;
use std::io::{stdin, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread::sleep;

use serde::{Deserialize, Serialize};
use threadpool::ThreadPool;

#[derive(Debug)]
struct Player {
    ip: IpAddr,
    name: String,
    color: Option<usize>,
}

impl Player {
    fn new(name: String, ip: IpAddr) -> Self {
        Player {
            name,
            color: None,
            ip,
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
    Ok(IpAddr),
    Fail(String, IpAddr),
    Move(usize, usize, Option<String>),
    Reset,
}

struct Game {
    players: Vec<Player>,
    active_player: Option<usize>,
    winner: Option<String>,
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

    fn winner_check(&mut self, player_id: usize) {
        self.horizontal_check(player_id);
        [14, 15, 16].map(|shift| self.shift_check(player_id, shift));
    }

    fn horizontal_check(&mut self, player_id: usize) {
        let player = &self.players[player_id];

        let rows = self.field.chunks(15);
        for row in rows {
            let mut win_line = vec![];
            let mut idx = 0;
            while idx < row.len() {
                let cell_color = row[idx];
                if cell_color == player.color.unwrap() {
                    win_line.push(idx);
                } else {
                    win_line = vec![];
                }
                if win_line.len() >= 5 {
                    self.winner = Some(player.name.clone());
                    return;
                }
                idx += 1;
            }
        }
    }

    fn shift_check(&mut self, player_id: usize, shift: usize) {
        let player = &self.players[player_id];
        let mut idx = 0;
        let mut win_line = vec![];
        let winner_color = player.color.unwrap();
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
                    self.winner = Some(player.name.clone());
                    return;
                }
                i += shift;
            }
            win_line = vec![];
            idx += 1;
        }
    }
}

fn handle_game_action(data: &[u8], game: Arc<RwLock<Game>>, player_ip: IpAddr) -> ServerResponse {
    let data = bincode::deserialize::<GameAction>(data);
    match data {
        Ok(GameAction::Reset) => {
            game.write().unwrap().reset();
            ServerResponse::Reset
        }

        Ok(GameAction::Connect(name)) => {
            let new_player = Player::new(name, player_ip);
            let mut game = game.write().unwrap();
            game.players.push(new_player);
            ServerResponse::Ok(player_ip)
        }

        Ok(GameAction::Move(move_id, name)) => {
            if game.read().unwrap().players.len() < 2 {
                return ServerResponse::Fail(
                    "Wait for a second player to connect".to_string(),
                    player_ip,
                );
            }

            let mut game = game.write().unwrap();
            let (player_id, second_player_id) = if game.players[0].name == name {
                (0_usize, 1_usize)
            } else {
                (1_usize, 0_usize)
            };

            match game.active_player {
                Some(active_player_id) => {
                    if player_id != game.active_player.unwrap() {
                        return ServerResponse::Fail("It's not your move".to_string(), player_ip);
                    };

                    if active_player_id == 0_usize {
                        game.active_player = Some(1)
                    } else {
                        game.active_player = Some(0)
                    };
                }
                None => {
                    game.players[player_id].color = Some(1_usize);
                    game.players[second_player_id].color = Some(2_usize);

                    game.active_player = Some(second_player_id);
                }
            }
            let player_color = game.players[player_id].color.unwrap();
            game.field[move_id] = player_color;
            game.winner_check(player_id);

            ServerResponse::Move(move_id, player_color, game.winner.clone())
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
    let listener = TcpListener::bind(address).unwrap();
    listener
        .set_nonblocking(true)
        .expect("Failed to initialize non-blocking");
    println!("Server listening on ip:port = {}", address);

    let game = Arc::new(RwLock::new(Game::new()));
    let mut clients: HashMap<IpAddr, TcpStream> = HashMap::new();
    let pool = ThreadPool::new(4);

    let (tx, rx): (Sender<ServerResponse>, Receiver<ServerResponse>) = channel();
    loop {
        if let Ok((mut stream, _socket_addr)) = listener.accept() {
            println!("Client {:?} connected", stream.peer_addr());

            let player_ip = stream.peer_addr().unwrap().ip();
            clients.insert(player_ip, stream.try_clone().unwrap());

            let game = Arc::clone(&game);
            // let mut stream_clone = stream.unwrap().try_clone();
            let tx = tx.clone();
            pool.execute(move || loop {
                let arc_game = Arc::clone(&game);

                let mut data = [0; 64];
                match stream.read(&mut data) {
                    Ok(size) => {
                        if size == 0 {
                            return;
                        }

                        let response = handle_game_action(&data, arc_game, player_ip);
                        tracing::error!("{:?}", response);
                        if let Err(e) = tx.send(response) {
                            tracing::error!("Sending message to a transmitter failed: {}", e)
                        };
                    }
                    Err(e) => {
                        println!("Data read error: {}", e);
                    }
                }
                sleep(time::Duration::from_millis(300));
            });
        }
        match rx.try_recv() {
            Ok(response) => match response {
                ServerResponse::Move(move_id, player_color, winner) => {
                    let resp =
                        bincode::serialize(&ServerResponse::Move(move_id, player_color, winner))
                            .unwrap();
                    for mut client in clients.values() {
                        tracing::error!("{:?}", resp);
                        client.write_all(&resp).unwrap();
                    }
                }
                ServerResponse::Reset => {
                    let resp = bincode::serialize(&ServerResponse::Reset).unwrap();
                    for mut client in clients.values() {
                        tracing::error!("{:?}", resp);
                        client.write_all(&resp).unwrap();
                    }
                }
                ServerResponse::Ok(player_ip) => {
                    let resp = bincode::serialize(&ServerResponse::Ok(player_ip)).unwrap();
                    clients
                        .get_mut(&player_ip)
                        .unwrap()
                        .write_all(&resp)
                        .unwrap();
                }
                ServerResponse::Fail(message, player_ip) => {
                    let resp =
                        bincode::serialize(&ServerResponse::Fail(message, player_ip)).unwrap();
                    clients
                        .get_mut(&player_ip)
                        .unwrap()
                        .write_all(&resp)
                        .unwrap();
                }
            },
            Err(e) => {
                tracing::error!("Failed to receive a value from the rx: {}", e);
            }
        }
        sleep(time::Duration::from_millis(100));
    }
}
