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
use std::collections::HashSet;

use log::{error, info, warn};
use thiserror::Error;

use crate::config;
use crate::metrics;
use crate::strategy;

#[derive(Error, Debug)]
pub enum CommandErr {
    #[error("Invalid argument parsed for command")]
    InvalidCommandArgument,
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
    let strategies = strategy::Strategy::from_discord_args(&mut args);

    if let Ok(strategies) = strategies {
        info!("Calculated strategy for user {}", msg.author.name);
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
    } else {
        warn!(
            "Bad input or help request for strat command, user {}",
            msg.author.name
        );
        msg.channel_id.send_message(ctx, |m| {
            m.content(format!("{}, try one of the examples below:\n\
            >>> **Usage:** `!strat <Race Length HH:MM or MMM> <Lap Time> <Fuel per Lap> <Fuel Capacity> [<Mandatory Pits> <Max Stint Length HH:MM>]`\n\
            **Example 1:** `!strat 2:24 2:18 3.44 120`\n\
            **Example 2:** `!strat 2:24 2:18 3.44 120 1 0:55`", msg.author.mention()));
            m
        }).await?;
    }

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
