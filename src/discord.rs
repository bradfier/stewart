use std::collections::HashSet;
use std::time::Duration;

use serenity::builder::CreateEmbed;
use serenity::client::{Client, Context};
use serenity::framework::standard::{
    help_commands,
    macros::{check, command, group, help, hook},
    Args, CheckResult, CommandGroup, CommandOptions, CommandResult, HelpOptions, StandardFramework,
};
use serenity::model::channel::{ChannelType, Message};
use serenity::model::id::UserId;
use serenity::prelude::*;

use log::{error, info, warn};
use thiserror::Error;

use crate::config;
use crate::metrics;
use crate::strategy;

#[derive(Error, Debug)]
enum CommandErr {
    #[error("Invalid argument parsed for command")]
    InvalidCommandArgument,
}

// Accept either minutes or HH:MM
fn parse_mins_or_hhmm(input: &str) -> Result<Duration, CommandErr> {
    if !input.contains(':') {
        let i = input
            .parse::<u32>()
            .map_err(|_| CommandErr::InvalidCommandArgument)?;
        return Ok(Duration::new(i as u64 * 60, 0));
    } else {
        let parts: Vec<&str> = input.split(':').collect();
        if parts.len() == 2 {
            let hours = parts[0].parse::<u32>().ok();
            let minutes = parts[1].parse::<u32>().ok();

            if let (Some(hours), Some(minutes)) = (hours, minutes) {
                let mins: u64 = (hours * 60 + minutes) as u64;
                return Ok(Duration::new(mins * 60, 0));
            }
        }
    }
    Err(CommandErr::InvalidCommandArgument)
}

fn parse_mmss(input: &str) -> Result<Duration, CommandErr> {
    let parts: Vec<&str> = input.split(':').collect();
    if parts.len() == 2 {
        let mins = parts[0].parse::<u32>().ok();
        let secs = parts[1].parse::<u32>().ok();

        if let (Some(mins), Some(secs)) = (mins, secs) {
            return Ok(Duration::new((secs + mins * 60) as u64, 0));
        }
    }
    Err(CommandErr::InvalidCommandArgument)
}

#[group]
#[only_in(guilds)]
#[prefix("strat")]
#[commands(strat_calc)]
#[default_command(strat_calc)]
struct Strat;

#[command]
#[aliases("calc")]
async fn strat_calc(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    // Ordering must be preserved
    let race_time = parse_mins_or_hhmm(&args.single::<String>()?)?;
    let lap_time = parse_mmss(&args.single::<String>()?)?;
    let fuel_per_lap = args.single::<f64>()?;
    let fuel_capacity = args.single::<u32>()?;

    // Optional args, mandatory pitstops and max stint time
    let mandatory_pits = if !args.is_empty() {
        Some(args.single::<u8>()?)
    } else {
        None
    };

    let permitted_max_stint_length = if !args.is_empty() {
        Some(parse_mins_or_hhmm(&args.single::<String>()?)?)
    } else {
        None
    };
    // End preserve ordering

    let strategy_input = strategy::StrategyInput {
        race_duration: race_time,
        avg_laptime: lap_time,
        fuel_per_lap,
        fuel_capacity,
        mandatory_pits,
        permitted_max_stint_length,
    };

    let strategies = strategy_input.calculate();

    let content = if strategies.len() == 1 {
        "We calculated one strategy for you.".to_string()
    } else {
        format!("We calculated {} strategies for you.", strategies.len())
    };

    msg.channel_id
        .send_message(ctx, |m| {
            m.content(msg.author.mention());
            m.embed(|e| {
                e.title("Strategy Calculator");
                e.description(content);
                for s in strategies {
                    e.field(s.discord_title(), s.as_discord_text(), true);
                }
                e
            });
            m
        })
        .await?;

    Ok(())
}

#[group]
#[only_in(guilds)]
#[commands(protest_channel)]
#[help_available(false)]
struct Protest;

#[check]
#[name = "InProtestChannel"]
async fn protest_channel_check(
    _ctx: &Context,
    msg: &Message,
    _: &mut Args,
    _: &CommandOptions,
) -> CheckResult {
    if config::CONFIG.protest_channels.contains(&msg.channel_id.0) {
        CheckResult::Success
    } else {
        warn!("Attempt to call !protest-channel in a non-whitelisted channel");
        CheckResult::new_log("This command is only permitted in certain channels.")
    }
}

#[command]
#[aliases("protest-channel")]
#[checks(InProtestChannel)]
async fn protest_channel(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let new_chan_name = args.single::<String>()?;
    let source_channel = msg.channel_id.to_channel(ctx).await?.guild();

    if let Some(source_channel) = source_channel {
        let guild_name = source_channel
            .guild_id
            .name(ctx)
            .await
            .unwrap_or_else(|| "Unknown".to_string());
        let target_category = source_channel.category_id;
        info!(
            "Creating channel \"{}\" for user {}",
            new_chan_name, msg.author.name
        );
        let new_chan = source_channel
            .guild_id
            .create_channel(ctx, |c| {
                let c = c.name(new_chan_name).kind(ChannelType::Text);
                if let Some(category) = target_category {
                    c.category(category)
                } else {
                    c
                }
            })
            .await?;

        let remaining_text = args.remains();

        if remaining_text.is_some() || !msg.embeds.is_empty() {
            info!("Sending intro message to channel");
            // Serenity only support setting one embed on an outbound message so we just take the
            // first from that which triggered us
            let embed: Option<CreateEmbed> = msg.embeds.first().cloned().map(Into::into);

            new_chan
                .send_message(ctx, |mut m| {
                    if let Some(text) = remaining_text {
                        m = m.content(text);
                        info!("Intro text: {}", text);
                    }
                    if let Some(embed) = embed {
                        m = m.set_embed(embed);
                    }
                    m
                })
                .await?;
        }

        metrics::PROTEST_CHANNELS_CREATED
            .with_label_values(&[&msg.author.name, &guild_name])
            .inc();
    } else {
        error!("Received a create-channel message from a non-guild context");
    }

    Ok(())
}

#[hook]
async fn after(ctx: &Context, msg: &Message, command_name: &str, command_result: CommandResult) {
    let success = match command_result {
        Ok(()) => true,
        Err(_) => false,
    };

    let guild_name = if let Some(gid) = msg.guild_id {
        gid.name(ctx).await.unwrap_or_else(|| "None".to_string())
    } else {
        "None".to_string()
    };

    metrics::COMMANDS_EXECUTED
        .with_label_values(&[
            command_name,
            &success.to_string(),
            &msg.author.name,
            &guild_name,
        ])
        .inc();

    // Update our current view of how many Guilds we're connected to
    if let Ok(guilds) = ctx.cache.current_user().await.guilds(&ctx).await {
        metrics::GUILDS_CONNECTED.set(guilds.len() as i64);
    }
}

#[help]
async fn my_help(
    context: &Context,
    msg: &Message,
    args: Args,
    help_options: &'static HelpOptions,
    groups: &[&'static CommandGroup],
    owners: HashSet<UserId>,
) -> CommandResult {
    let _ = help_commands::with_embeds(context, msg, args, help_options, groups, owners).await;
    Ok(())
}

pub async fn create_client(token: &str) -> Client {
    let framework = StandardFramework::new()
        .configure(|c| c.prefix("!").ignore_webhooks(false).ignore_bots(false))
        .help(&MY_HELP)
        .group(&PROTEST_GROUP)
        .group(&STRAT_GROUP)
        .after(after);

    // Login with a bot token from the environment
    let client = Client::builder(token)
        .framework(framework)
        .await
        .expect("Error creating client");

    client
}
