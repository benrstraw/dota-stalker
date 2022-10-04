use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::sync::Arc;
use chrono::Utc;
use rmp_serde::{decode, encode};
use serenity::builder::CreateEmbed;
use serenity::CacheAndHttp;
use serenity::model::channel::Message;
use serenity::model::id::{ChannelId, UserId};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use crate::gsi::JsonKV;

struct Game {
	matchid: u64,
	message: Message,
}

type SteamId = u64;

#[derive(Debug)]
pub enum BotRequest {
	BindChannel {
		channel: ChannelId,
		resp: oneshot::Sender<Result<(), ()>>
	}
}

pub struct Bot {
	bot_req_rx: mpsc::Receiver<BotRequest>,
	gsi_rx: mpsc::Receiver<JsonKV>,
	cah: Arc<CacheAndHttp>,
	games: HashMap<SteamId, Game>,
	channels: HashSet<ChannelId>,
	users: HashMap<SteamId, UserId>,
	tracks: HashMap<UserId, HashSet<ChannelId>>,
}

impl Bot {
	pub fn new(cah: Arc<CacheAndHttp>, bot_req_rx: mpsc::Receiver<BotRequest>, gsi_rx: mpsc::Receiver<JsonKV>) -> Self {
		let channels_file = File::open("channels.dat");
		let channels = if channels_file.is_ok() {
			let res = decode::from_read(channels_file.unwrap());
			if res.is_ok() {
				res.unwrap()
			} else {
				log::error!("Error decoding data from channels.dat! Using an empty list!");
				HashSet::new()
			}
		} else {
			log::warn!("Could not open channels.dat! (First run?)");
			HashSet::new()
		};

		for channel in &channels {
			log::debug!("Loaded binding to channel {}", channel);
		}

		return Bot {
			bot_req_rx,
			gsi_rx,
			cah,
			games: HashMap::new(),
			channels,
			users: HashMap::new(),
			tracks: HashMap::new(),
		}
	}

	pub async fn run(mut self) {
		log::info!("Starting bot handler!");

		loop {
			tokio::select! {
	            Some(data) = self.bot_req_rx.recv() => self.handle_bot_request(data).await,
	            Some(data) = self.gsi_rx.recv() => self.handle_game_data(data).await,
	            else => { break }
	        };
		}

		log::warn!("Bot handler killed!");
	}

	pub fn write_data(&mut self) {
		let channels_file = File::create("channels.dat");
		encode::write(&mut channels_file.unwrap(), &self.channels).unwrap();
	}

	pub async fn handle_bot_request(&mut self, data: BotRequest) {
		match data {
			BotRequest::BindChannel { channel, resp } => {
				self.channels.insert(channel);
				self.write_data();
				resp.send(Ok(())).unwrap();
			}
		};
	}

	pub async fn handle_game_data(&mut self, data: JsonKV) {
		log::debug!("{:#?}", data);

		let map = data.get("map");
		let player = data.get("player");

		if map.is_some() && player.is_some() {
			let map = map.unwrap().as_object().unwrap();
			let player = player.unwrap().as_object().unwrap();
			let steamid: u64 = player.get("steamid").unwrap().as_str().unwrap().parse().unwrap();
			let customgamename = map.get("customgamename");
			let state = map.get("game_state");
			if state.is_some() && (customgamename.is_none() || customgamename.unwrap().as_str().unwrap().is_empty()) {
				let state = state.unwrap().as_str().unwrap();
				if state == "DOTA_GAMERULES_STATE_PRE_GAME" || state == "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS" || state == "DOTA_GAMERULES_STATE_POST_GAME" {
					log::trace!("Found an in-progress match!");
					let matchid: u64 = map.get("matchid").unwrap().as_str().unwrap().parse().unwrap();
					if self.games.contains_key(&steamid) {
						if self.games.get(&steamid).unwrap().matchid == matchid {
							let game = self.games.get_mut(&steamid).unwrap();
							game.message.edit(&self.cah.http, |a| {
								a.embed(|b| {
									build_message(b, data)
								})
							}).await.unwrap();
						} else {
							log::info!("Found an old match for player {} with match ID {}. Updating to new match ID {}.", steamid, self.games.get(&steamid).unwrap().matchid, matchid);

							// let message = CHANNEL.send_message(&self.cah.http, |a| {
							// 	a.embed(|b| {
							// 		build_message(b, data)
							// 	})
							// }).await.unwrap();

							// let game = Game {
							// 	matchid,
							// 	message,
							// };

							// self.games.insert(steamid, game);
						}
					} else {
						log::info!("Creating new match for player {} with match ID {}.", steamid, matchid);

						// let message = CHANNEL.send_message(&self.cah.http, |a| {
						// 	a.embed(|b| {
						// 		build_message(b, data)
						// 	})
						// }).await.unwrap();
						//
						// let game = Game {
						// 	matchid,
						// 	message,
						// };
						//
						// self.games.insert(steamid, game);
					};
				}
			}
		}
	}
}

pub fn build_message(e: &mut CreateEmbed, data: JsonKV) -> &mut CreateEmbed {
	let map = data.get("map").unwrap().as_object().unwrap();
	let player = data.get("player").unwrap().as_object().unwrap();
	let hero = data.get("hero").unwrap().as_object().unwrap();

	let player_name = player.get("name").unwrap().as_str().unwrap();
	let player_team = player.get("team_name").unwrap().as_str().unwrap();
	e.title(format!("{} is playing a match on {}!", player_name, uppercase_first_letter(player_team)));

	let clock_time = map.get("clock_time").unwrap().as_i64().unwrap();
	e.field("Time", clock_time, true);

	let radiant_score = map.get("radiant_score").unwrap().as_u64().unwrap();
	let dire_score = map.get("dire_score").unwrap().as_u64().unwrap();
	e.field("Radiant / Dire", format!("{}/{}", radiant_score, dire_score), true);

	let hero_name = hero.get("name").unwrap().as_str().unwrap();
	e.field("Hero", hero_name, false);

	let hero_level = hero.get("level").unwrap().as_i64().unwrap();
	e.field("Level", hero_level, true);

	let gold = player.get("gold").unwrap().as_i64().unwrap();
	e.field("Gold", gold, true);

	e.field("\u{200b}", "\u{200b}", false); // New line.

	let health = hero.get("health").unwrap().as_i64().unwrap();
	let max_health = hero.get("max_health").unwrap().as_i64().unwrap();
	e.field("Health", format!("{}/{}", health, max_health), true);

	let mana = hero.get("mana").unwrap().as_i64().unwrap();
	let max_mana = hero.get("max_mana").unwrap().as_i64().unwrap();
	e.field("Mana", format!("{}/{}", mana, max_mana), true);

	e.field("\u{200b}", "\u{200b}", false); // New line.

	let kills = player.get("kills").unwrap().as_u64().unwrap();
	let deaths = player.get("deaths").unwrap().as_u64().unwrap();
	let assists = player.get("assists").unwrap().as_u64().unwrap();
	e.field("K/D/A", format!("{}/{}/{}", kills, deaths, assists), true);

	let last_hits = player.get("last_hits").unwrap().as_u64().unwrap();
	let denies = player.get("denies").unwrap().as_u64().unwrap();
	e.field("CS/DN", format!("{}/{}", last_hits, denies), true);

	let xpm = player.get("xpm").unwrap().as_u64().unwrap();
	let gpm = player.get("gpm").unwrap().as_u64().unwrap();
	e.field("XPM/GPM", format!("{}/{}", xpm, gpm), true);

	let matchid: u64 = map.get("matchid").unwrap().as_str().unwrap().parse().unwrap();
	e.footer(|f| {
		f.text(format!("Match ID: {}", matchid))
	});

	e.timestamp(Utc::now());

	return e;
}

fn uppercase_first_letter(s: &str) -> String {
	let mut c = s.chars();
	match c.next() {
		None => String::new(),
		Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
	}
}