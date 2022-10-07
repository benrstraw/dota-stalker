use serenity::async_trait;
use serenity::client::{Context, EventHandler};
use serenity::model::application::command::{Command, CommandOptionType};
use serenity::model::application::interaction::{Interaction, MessageFlags};
use serenity::model::application::interaction::InteractionResponseType::ChannelMessageWithSource;
use serenity::model::gateway::Ready;
use serenity::model::id::ChannelId;
use serenity::model::Permissions;
use serenity::prelude::TypeMapKey;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use crate::bot::{BotRequest, SteamId};

pub struct Events;

pub struct DiscordData {
	pub bot_req_tx: mpsc::Sender<BotRequest>,
}

pub struct DiscordKey;

impl TypeMapKey for DiscordKey {
	type Value = DiscordData;
}

#[async_trait]
impl EventHandler for Events {
	async fn ready(&self, ctx: Context, ready: Ready) {
		info!("{} is connected!", ready.user.name);
		Command::set_global_application_commands(&ctx.http, |commands| {
			commands
				.create_application_command(|command| {
					command
						.name("register")
						.description("Register with the events to set up game tracking.")
						.create_option(|option| {
							option
								.name("steamid")
								.description("Your SteamID64, which can be found at steamid.io")
								.kind(CommandOptionType::String)
								.required(false)
						})
				})
				.create_application_command(|command| {
					command
						.name("bind")
						.description("Bind the Dota Stalker output for this server to a given channel.")
						.default_member_permissions(Permissions::ADMINISTRATOR)
						.dm_permission(false)
						.create_option(|option| {
							option
								.name("channel")
								.description("Channel to bind the bot to.")
								.kind(CommandOptionType::Channel)
								.required(true)
						})
				})
				.create_application_command(|command| {
					command
						.name("track")
						.description("Start tracking your Dota matches in this channel.")
						.create_option(|option| {
							option
								.name("disable")
								.description("Set this to true to STOP tracking in this channel.")
								.kind(CommandOptionType::Boolean)
								.required(false)
						})
				})
		}).await.unwrap();
	}

	async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
		if let Interaction::ApplicationCommand(command) = interaction {
			match command.guild_id {
				None => {
					command.create_interaction_response(&ctx, |f| {
						f.kind(ChannelMessageWithSource);
						f.interaction_response_data(|g| {
							g.content("You cannot DM commands to this events, please send the command from inside a server!");
							g.flags(MessageFlags::EPHEMERAL)
						})
					}).await.unwrap();
				}
				Some(_gid) => {
					match command.data.name.as_str() {
						"register" => {
							log::trace!("Received track request from {} in channel {}", command.user.id, command.channel_id);

							let steam_id: Option<SteamId> = match command.data.options.get(0) {
								None => None,
								Some(x) => {
									match &x.value {
										None => None,
										Some(x) => {
											match x.as_str() {
												None => None,
												Some(x) => {
													match x.parse() {
														Ok(steam_id) => Some(steam_id),
														Err(_) => None,
													}
												}
											}
										}
									}
								}
							};

							let steam_id = match steam_id {
								None => {
									command.create_interaction_response(&ctx, |f| {
										f.kind(ChannelMessageWithSource);
										f.interaction_response_data(|g| {
											g.content("Invalid SteamID! Make sure to use the SteamID64.");
											g.flags(MessageFlags::EPHEMERAL)
										})
									}).await.unwrap();
									return;
								}
								Some(steam_id) => steam_id,
							};

							log::trace!("Attempting to register {} with SteamID {}", command.user.id, steam_id);

							let data = ctx.data.read().await;
							let data = data.get::<DiscordKey>().unwrap();
							let (tx, rx) = oneshot::channel();
							let request = BotRequest::RegisterUser {
								user: command.user.id,
								steam_id,
								resp: tx,
							};

							log::trace!("Sending bot request");

							data.bot_req_tx.send(request).await.unwrap();

							let resp = rx.await.unwrap();

							log::trace!("Received bot response");

							match resp {
								Ok(resp) => {
									command.create_interaction_response(&ctx, |f| {
										f.kind(ChannelMessageWithSource);
										f.interaction_response_data(|g| {
											g.content(format!("Auth token: {}", resp));
											g.flags(MessageFlags::EPHEMERAL)
										})
									}).await.unwrap();
								}
								Err(_) => {
									command.create_interaction_response(&ctx, |f| {
										f.kind(ChannelMessageWithSource);
										f.interaction_response_data(|g| {
											g.content("There was an unexpected error!");
											g.flags(MessageFlags::EPHEMERAL)
										})
									}).await.unwrap();
								}
							}
						}
						"bind" => {
							log::trace!("Received bind request from {}", command.user.id);

							let member = command.member.as_ref().unwrap();
							let perms = member.permissions.as_ref().unwrap();

							if !perms.contains(Permissions::ADMINISTRATOR) {
								command.create_interaction_response(&ctx, |f| {
									f.kind(ChannelMessageWithSource);
									f.interaction_response_data(|g| {
										g.content("Only server Administrators can bind this bot!");
										g.flags(MessageFlags::EPHEMERAL)
									})
								}).await.unwrap();

								return;
							}

							let channel = command.data.options.get(0).unwrap().value.as_ref().unwrap();
							let channel: u64 = channel.as_str().unwrap().parse().unwrap();
							let channel = ChannelId::from(channel);

							log::trace!("Attempting to bind to {}", channel);

							let data = ctx.data.read().await;
							let data = data.get::<DiscordKey>().unwrap();
							let (tx, rx) = oneshot::channel();
							let request = BotRequest::BindChannel {
								channel,
								resp: tx,
							};

							log::trace!("Sending bot request");

							data.bot_req_tx.send(request).await.unwrap();

							let resp = rx.await.unwrap();

							log::trace!("Received bot response");

							if resp.is_ok() {
								command.create_interaction_response(&ctx, |f| {
									f.kind(ChannelMessageWithSource);
									f.interaction_response_data(|g| {
										g.content(format!("Successfully bound to {}", channel));
										g.flags(MessageFlags::EPHEMERAL)
									})
								}).await.unwrap();
							} else {
								if resp.is_ok() {
									command.create_interaction_response(&ctx, |f| {
										f.kind(ChannelMessageWithSource);
										f.interaction_response_data(|g| {
											g.content(format!("Error binding to {}!", channel));
											g.flags(MessageFlags::EPHEMERAL)
										})
									}).await.unwrap();
								}
							}
						}
						"track" => {
							log::trace!("Received track request from {} in channel {}", command.user.id, command.channel_id);

							let disable = match command.data.options.get(0) {
								None => false,
								Some(x) => {
									match &x.value {
										None => false,
										Some(x) => {
											match x.as_bool() {
												None => false,
												Some(x) => {
													x
												}
											}
										}
									}
								}
							};

							log::trace!("Attempting to add track for {} in {}", command.user.id, command.channel_id);

							let data = ctx.data.read().await;
							let data = data.get::<DiscordKey>().unwrap();
							let (tx, rx) = oneshot::channel();
							let request = if disable {
								BotRequest::RemoveTrack {
									user: command.user.id,
									channel: command.channel_id,
									resp: tx,
								}
							} else {
								BotRequest::AddTrack {
									user: command.user.id,
									channel: command.channel_id,
									resp: tx,
								}
							};

							log::trace!("Sending bot request");

							data.bot_req_tx.send(request).await.unwrap();

							let resp = rx.await.unwrap();

							log::trace!("Received bot response");

							if resp.is_ok() {
								command.create_interaction_response(&ctx, |f| {
									f.kind(ChannelMessageWithSource);
									f.interaction_response_data(|g| {
										g.content("Success!");
										g.flags(MessageFlags::EPHEMERAL)
									})
								}).await.unwrap();
							} else {
								if resp.is_ok() {
									command.create_interaction_response(&ctx, |f| {
										f.kind(ChannelMessageWithSource);
										f.interaction_response_data(|g| {
											g.content("The bot is not bound to this channel! Please switch to a valid channel.");
											g.flags(MessageFlags::EPHEMERAL)
										})
									}).await.unwrap();
								}
							}
						}
						_ => unreachable!(),
					}
				}
			}
		}
	}
}