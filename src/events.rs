use serenity::async_trait;
use serenity::client::{Context, EventHandler};
use serenity::model::application::command::{Command, CommandOptionType};
use serenity::model::application::interaction::{Interaction, MessageFlags};
use serenity::model::application::interaction::InteractionResponseType::ChannelMessageWithSource;
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;

const GUILD_ID: GuildId = GuildId(779898251699814430);

pub struct Events;

#[async_trait]
impl EventHandler for Events {
	async fn ready(&self, ctx: Context, ready: Ready) {
		info!("{} is connected!", ready.user.name);
		Command::set_global_application_commands(&ctx.http, |x| x).await.unwrap();

		GUILD_ID
			.set_application_commands(&ctx.http, |commands| {
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
			})
			.await.unwrap();
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
				Some(gid) => {

				}
			}
		}
	}
}