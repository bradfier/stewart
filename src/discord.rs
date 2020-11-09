use log::error;
use serenity::builder::CreateEmbed;
use serenity::client::{Client, Context};
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult, StandardFramework,
};
use serenity::model::channel::{ChannelType, Message};

use log::info;

#[group]
#[only_in(guilds)]
#[commands(protest_channel)]
struct General;

#[command]
#[aliases("protest-channel")]
async fn protest_channel(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let new_chan_name = args.single::<String>()?;
    let source_channel = msg.channel_id.to_channel(ctx).await?.guild();

    if let Some(source_channel) = source_channel {
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
                    }
                    if let Some(embed) = embed {
                        m = m.set_embed(embed);
                    }
                    m
                })
                .await?;
        }
    } else {
        error!("Received a create-channel message from a non-guild context");
    }

    Ok(())
}

pub async fn create_client(token: &str) -> Client {
    let framework = StandardFramework::new()
        .configure(|c| c.prefix("!").ignore_webhooks(false).ignore_bots(false))
        .group(&GENERAL_GROUP);

    // Login with a bot token from the environment
    let client = Client::builder(token)
        .framework(framework)
        .await
        .expect("Error creating client");

    client
}
