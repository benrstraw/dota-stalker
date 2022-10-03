#[macro_use]
extern crate log;

use std::env;

use fern::colors::{Color, ColoredLevelConfig};
use serenity::Client;
use serenity::prelude::GatewayIntents;
use tokio::sync::mpsc;
use crate::bot::Bot;

mod events;
mod gsi;
mod bot;

const GSI_URI: &str = "127.0.0.1:3682";

// fn echo_handler(gs: GameState) {
//     println!("{}", gs);
// }

#[tokio::main]
async fn main() {
    // Load the .env file to populate BOT_TOKEN and APP_ID
    dotenv::dotenv().ok();

    let token = env::var("BOT_TOKEN").expect("Expected a token in the environment");

    let application_id: u64 = env::var("APP_ID")
        .expect("Expected an application id in the environment")
        .parse()
        .expect("application id is not a valid id");

    setup_logger();

    let gsi = gsi::Server::new(GSI_URI);

    let mut client = Client::builder(token, GatewayIntents::non_privileged())
        .event_handler(events::Events)
        .application_id(application_id)
        .await
        .expect("Error creating client");

    let (tx, rx) = mpsc::channel(10);
    let bot = Bot::new(client.cache_and_http.clone(), rx);

    info!("Initializing Dota Stalker...");

    tokio::spawn(async move {
        bot.run().await;
    });

    tokio::spawn(async move {
        gsi.run(tx).await;
    });

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }

    info!("Goodbye!");
}

fn setup_logger() {
    let colors_line = ColoredLevelConfig::new()
        .error(Color::BrightRed)
        .warn(Color::BrightYellow)
        .info(Color::BrightWhite)
        .debug(Color::White)
        .trace(Color::BrightBlack);

    let colors_level = colors_line.clone()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::BrightGreen)
        .debug(Color::BrightCyan)
        .trace(Color::Black);

    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{color_line}[{date}][{target}][{level}{color_line}] {message}\x1B[0m",
                color_line = format_args!(
                    "\x1B[{}m",
                    colors_line.get_color(&record.level()).to_fg_str()
                ),
                date = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                target = record.target(),
                level = colors_level.color(record.level()),
                message = message,
            ));
        })
        .level(log::LevelFilter::Warn)
        .level_for("dota_stalker", log::LevelFilter::Trace)
        .chain(std::io::stdout())
        .chain(fern::log_file("stalker.log").unwrap())
        .apply()
        .unwrap();
}