use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Debug)]
pub struct Player {
  pub  ip: IpAddr,
  pub  name: String,
  pub  color: Option<usize>,
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
enum Command {
    Connect { username: String },
    Move { move_id: usize, username: String },
    Reset,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ServerResponse {
    Ok {
        player_ip: IpAddr,
    },
    Fail {
        message: String,
        player_ip: IpAddr,
    },
    Move {
        move_id: usize,
        color: usize,
        winner: Option<String>,
    },
    Reset,
}

pub struct Game {
    pub players: Vec<Player>,
    pub active_player: Option<usize>,
    pub winner: Option<String>,
    pub field: [usize; 255],
}

impl Game {
    pub fn new() -> Self {
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

    pub fn handle_action(&mut self, data: &[u8], player_ip: IpAddr) -> ServerResponse {
        let data = bincode::deserialize::<Command>(data);
        match data {
            Ok(Command::Reset) => {
                self.reset();
                ServerResponse::Reset
            }

            Ok(Command::Connect { username }) => {
                let new_player = Player::new(username, player_ip);
                self.players.push(new_player);
                ServerResponse::Ok { player_ip }
            }

            Ok(Command::Move { move_id, username }) => {
                if self.players.len() < 2 {
                    return ServerResponse::Fail {
                        message: "Wait for a second player to connect".to_string(),
                        player_ip,
                    };
                }

                let (player_id, second_player_id) = if self.players[0].name == username {
                    (0_usize, 1_usize)
                } else {
                    (1_usize, 0_usize)
                };

                match self.active_player {
                    Some(active_player_id) => {
                        if player_id != self.active_player.unwrap() {
                            return ServerResponse::Fail {
                                message: "It's not your move".to_string(),
                                player_ip,
                            };
                        };

                        if active_player_id == 0_usize {
                            self.active_player = Some(1)
                        } else {
                            self.active_player = Some(0)
                        };
                    }
                    None => {
                        self.players[player_id].color = Some(1_usize);
                        self.players[second_player_id].color = Some(2_usize);

                        self.active_player = Some(second_player_id);
                    }
                }
                let color = self.players[player_id].color.unwrap();
                self.field[move_id] = color;
                self.winner_check(player_id);

                ServerResponse::Move {
                    move_id,
                    color,
                    winner: self.winner.clone(),
                }
            }
            Err(e) => {
                panic!("{}", e)
            }
        }
    }
}
