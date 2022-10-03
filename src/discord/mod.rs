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
use crate::bot::BotRequest;

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
								.description("Steam ID")
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
						"register" => todo!(),
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
						_ => unreachable!(),
					}
				}
			}
		}
	}
}