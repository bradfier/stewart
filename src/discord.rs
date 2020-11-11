use log::error;
use serenity::builder::CreateEmbed;
use serenity::client::{Client, Context};
use serenity::framework::standard::{
    macros::{check, command, group, hook},
    Args, CheckResult, CommandOptions, CommandResult, StandardFramework,
};
use serenity::model::channel::{ChannelType, Message};

use log::{info, warn};

use crate::config;
use crate::metrics;

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

pub async fn create_client(token: &str) -> Client {
    let framework = StandardFramework::new()
        .configure(|c| c.prefix("!").ignore_webhooks(false).ignore_bots(false))
        .group(&PROTEST_GROUP)
        .after(after);

    // Login with a bot token from the environment
    let client = Client::builder(token)
        .framework(framework)
        .await
        .expect("Error creating client");

    client
}
