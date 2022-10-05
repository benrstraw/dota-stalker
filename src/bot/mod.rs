use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::str::FromStr;
use std::sync::Arc;
use chrono::Utc;
use dota::components::{DotaGameRulesState, GameState, Map};
use dota::components::heroes::{GameHeroes, Hero};
use dota::components::players::{GamePlayers, PlayerInformation};
use rmp_serde::{decode, encode};
use rusty_ulid::Ulid;
use serenity::builder::CreateEmbed;
use serenity::CacheAndHttp;
use serenity::model::channel::Message;
use serenity::model::id::{ChannelId, UserId};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use serde::{Serialize, Deserialize};

pub type SteamId = u64;

struct GamePosts {
	match_id: u64,
	messages: Vec<Message>,
}

#[derive(Serialize, Deserialize, Debug, Eq, Hash, PartialEq, Copy, Clone)]
struct UserInfo {
	token: Ulid,
	steam_id: SteamId,
}

#[derive(Debug)]
pub enum BotRequest {
	RegisterUser {
		user: UserId,
		steam_id: SteamId,
		resp: oneshot::Sender<Result<Ulid, ()>>
	},
	BindChannel {
		channel: ChannelId,
		resp: oneshot::Sender<Result<(), ()>>
	},
	AddTrack {
		user: UserId,
		channel: ChannelId,
		resp: oneshot::Sender<Result<(), ()>>
	},
	RemoveTrack {
		user: UserId,
		channel: ChannelId,
		resp: oneshot::Sender<Result<(), ()>>
	},
}

#[derive(Serialize, Deserialize)]
struct SaveData {
	channels: HashSet<ChannelId>,
	users: HashMap<UserInfo, UserId>,
	tracks: HashMap<UserId, HashSet<ChannelId>>,
}

impl SaveData {
	fn new() -> Self {
		Self {
			channels: HashSet::new(),
			users: HashMap::new(),
			tracks: HashMap::new(),
		}
	}
}

struct GameData {
	map: Map,
	player_info: PlayerInformation,
	hero: Hero,
	match_id: u64,
	user_info: UserInfo,
	user_id: UserId,
}

pub struct Bot {
	bot_req_rx: mpsc::Receiver<BotRequest>,
	gsi_rx: mpsc::Receiver<GameState>,
	cah: Arc<CacheAndHttp>,
	games: HashMap<SteamId, GamePosts>,
	save: SaveData,
}

impl Bot {
	pub fn new(cah: Arc<CacheAndHttp>, bot_req_rx: mpsc::Receiver<BotRequest>, gsi_rx: mpsc::Receiver<GameState>) -> Self {
		let data_file = File::open("stalker.dat");
		let save = if data_file.is_ok() {
			let res = decode::from_read(data_file.unwrap());
			if res.is_ok() {
				res.unwrap()
			} else {
				log::error!("Error decoding data from stalker.dat! Using an empty dataset!");
				SaveData::new()
			}
		} else {
			log::warn!("Could not open stalker.dat! (First run?)");
			SaveData::new()
		};

		for channel in &save.channels {
			log::debug!("Loaded binding to channel {}", channel);
		}

		for user in &save.users {
			log::debug!("Loaded user {:#?}", user);
		}

		for track in &save.tracks {
			log::debug!("Loaded tracks for user {}: {:#?}", track.0, track.1);
		}

		return Bot {
			bot_req_rx,
			gsi_rx,
			cah,
			games: HashMap::new(),
			save,
		}
	}

	pub async fn run(mut self) {
		log::info!("Starting bot handler!");

		loop {
			tokio::select! {
	            Some(data) = self.bot_req_rx.recv() => self.handle_bot_request(data).await,
	            Some(data) = self.gsi_rx.recv() => self.handle_game_state(data).await,
	            else => { break }
	        };
		}

		log::warn!("Bot handler killed!");
	}

	pub fn write_data(&mut self) {
		let data_file = File::create("stalker.dat");
		encode::write(&mut data_file.unwrap(), &self.save).unwrap();
	}

	pub async fn handle_bot_request(&mut self, data: BotRequest) {
		match data {
			BotRequest::BindChannel { channel, resp } => {
				self.save.channels.insert(channel);
				self.write_data();
				resp.send(Ok(())).unwrap();
			}
			BotRequest::AddTrack { user, channel, resp } => {
				if self.save.channels.contains(&channel) {
					match self.save.tracks.get_mut(&user) {
						None => {
							let mut set = HashSet::new();
							set.insert(channel);
							self.save.tracks.insert(user, set);
						}
						Some(tracks) => {
							tracks.insert(channel);
							self.write_data();
						}
					}
					resp.send(Ok(())).unwrap();
				} else {
					resp.send(Err(())).unwrap();
				}
			}
			BotRequest::RemoveTrack { user, channel, resp } => {
				match self.save.tracks.get_mut(&user) {
					None => {}
					Some(tracks) => {
						let removed = tracks.remove(&channel);
						if removed {
							if tracks.is_empty() {
								self.save.tracks.remove(&user);
							}
							self.write_data();
						}
					}
				}
				resp.send(Ok(())).unwrap();
			}
			BotRequest::RegisterUser { user, steam_id, resp } => {
				let user_info = UserInfo {
					token: Ulid::generate(),
					steam_id
				};

				self.save.users.insert(user_info, user);
				self.write_data();
				resp.send(Ok(user_info.token)).unwrap();
			}
		};
	}

	pub async fn handle_game_state(&mut self, state: GameState) {
		let hero = match state.heroes {
			None => return,
			Some(heroes) => {
				match heroes {
					GameHeroes::Spectating(_) => return,
					GameHeroes::Playing(hero) => hero,
				}
			}
		};

		let map = match state.map {
			None => return,
			Some(x) => x,
		};

		if !map.customgamename.is_empty() {
			return;
		}

		match map.game_state {
			DotaGameRulesState::StrategyTime | DotaGameRulesState::PreGame | DotaGameRulesState::InProgress | DotaGameRulesState::PostGame => (),
			_ => return,
		}

		let token = match state.auth {
			None => return,
			Some(auth) => {
				match auth.token {
					None => return,
					Some(token) => match Ulid::from_str(token.as_str()) {
						Ok(x) => x,
						Err(_) => return,
					}
				}
			},
		};

		let player_info = match state.players {
			None => return,
			Some(players) => {
				match players {
					GamePlayers::Spectating(_) => return,
					GamePlayers::Playing(player) => {
						player
					}
				}
			}
		};

		let steam_id = match player_info.steamid.parse() {
			Ok(x) => x,
			Err(_) => return,
		};

		let user_info = UserInfo {
			token,
			steam_id,
		};

		match self.save.users.get(&user_info) {
			None => return,
			Some(user_id) => {
				let match_id = match map.match_id.parse() {
					Ok(match_id) => match_id,
					Err(_) => return,
				};

				log::debug!("Found an in-progress match for a user we track!\nUser: {:?}\nMatch: {}", user_info, match_id);

				let game_data = GameData {
					map,
					player_info,
					hero,
					match_id,
					user_info,
					user_id: user_id.clone(),
				};

				let game = self.games.get_mut(&steam_id);
				match game {
					Some(game) => {
						if game.match_id == match_id {
							for message in &mut game.messages {
								message.edit(&self.cah.http, |a| {
									a.embed(|b| {
										build_message(b, &game_data)
									})
								}).await.unwrap();
							}
						} else {
							log::debug!("Found an old match for user {:?} with match ID {}. Updating to new match ID {}.", user_info, game.match_id, match_id);
							self.new_messages(game_data).await;
						}
					}
					None => {
						log::info!("Creating new match for user {:?} with match ID {}.", user_info, match_id);
						self.new_messages(game_data).await;
					}
				}
			}
		}
	}

	async fn new_messages(&mut self, game_data: GameData) {
		match self.save.tracks.get(&game_data.user_id) {
			None => {
				log::trace!("User {:?} has no tracked channels", game_data.user_info);
				self.games.remove(&game_data.user_info.steam_id);
				return;
			}
			Some(tracks) => {
				let mut messages = Vec::new();

				for channel in tracks {
					let message = channel.send_message(&self.cah.http, |a| {
						a.embed(|b| {
							build_message(b, &game_data)
						})
					}).await;

					match message {
						Ok(message) => messages.push(message),
						Err(err) => log::error!("Error sending new message! `{}`", err),
					}
				}

				let game_posts = GamePosts {
					match_id: game_data.match_id,
					messages
				};

				self.games.insert(game_data.user_info.steam_id, game_posts);
			}
		}
	}
}

fn build_message<'a, 'b>(e: &'a mut CreateEmbed, data: &'b GameData) -> &'a mut CreateEmbed {
	let map = &data.map;
	let player = &data.player_info;
	let hero = &data.hero;

	e.title(format!("{} is playing a match on {}!", player.name, player.team_name));

	e.field("Time", map.clock_time, true);

	e.field("Radiant / Dire", format!("{}/{}", map.radiant_score, map.dire_score), true);

	e.field("Hero", hero.name.as_ref().unwrap(), false);

	if hero.level.is_some() {
		e.field("Level", hero.level.unwrap(), true);
	}

	e.field("Gold", player.gold, true);

	e.field("\u{200b}", "\u{200b}", false); // New line.

	if hero.health.is_some() && hero.max_health.is_some() && hero.mana.is_some() && hero.max_mana.is_some() {
		e.field("Health", format!("{}/{}", hero.health.unwrap(), hero.max_health.unwrap()), true);
		e.field("Mana", format!("{}/{}", hero.mana.unwrap(), hero.max_mana.unwrap()), true);
	}

	e.field("\u{200b}", "\u{200b}", false); // New line.

	e.field("K/D/A", format!("{}/{}/{}", player.kills, player.deaths, player.assists), true);

	e.field("CS/DN", format!("{}/{}", player.last_hits, player.denies), true);

	e.field("XPM/GPM", format!("{}/{}", player.xpm, player.gpm), true);

	e.footer(|f| {
		f.text(format!("Match ID: {}", data.match_id))
	});

	e.timestamp(Utc::now());

	return e;
}