use chrono::{DateTime, Local, Timelike, Utc};
use chrono_tz::America::Toronto;
use std::env;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use serenity::all::*;
use serenity::async_trait;
use serenity::builder::{CreateActionRow, CreateButton};

const CONTROL_CHANNEL_NAME: &str = "light-controls";

fn get_env_var(key: &str) -> String {
    // First try to get from .env file
    if let Ok(val) = dotenv::var(key) {
        return val;
    }
    // Fall back to system environment variables
    env::var(key).unwrap_or_else(|_| panic!("Expected {key} in environment"))
}

#[derive(Clone)]
struct Handler {
    control_channel: Arc<RwLock<Option<ChannelId>>>,
    kasa_device_ip: String,
    kasa_username: String,
    kasa_password: String,
    kasa_dir: String,
}

impl Handler {
    fn new() -> Self {
        let kasa_device_ip = get_env_var("KASA_DEVICE_IP");
        let kasa_username = get_env_var("KASA_USERNAME");
        let kasa_password = get_env_var("KASA_PASSWORD");
        let kasa_dir = get_env_var("KASA_DIR");

        Self {
            control_channel: Arc::new(RwLock::new(None)),
            kasa_device_ip,
            kasa_username,
            kasa_password,
            kasa_dir,
        }
    }

    async fn setup_control_channel(&self, ctx: &Context) {
        let guilds: Vec<GuildInfo> = ctx.http.get_guilds(None, None).await.unwrap_or_default();

        for guild in guilds {
            let guild_id = guild.id;
            // Delete existing control channel if it exists
            if let Ok(channels) = guild_id.channels(&ctx.http).await {
                for (channel_id, channel) in channels {
                    if channel.name == CONTROL_CHANNEL_NAME {
                        if let Err(e) = channel_id.delete(&ctx.http).await {
                            error!("Failed to delete old control channel: {:?}", e);
                        }
                    }
                }
            }

            // Create new control channel
            match guild_id
                .create_channel(
                    &ctx.http,
                    CreateChannel::new(CONTROL_CHANNEL_NAME).kind(ChannelType::Text),
                )
                .await
            {
                Ok(channel) => {
                    let mut control_channel = self.control_channel.write().await;
                    *control_channel = Some(channel.id);

                    // Create the control message with buttons
                    if let Err(why) = channel
                        .send_message(
                            &ctx.http,
                            CreateMessage::new()
                                .content("Light Controls")
                                .components(vec![CreateActionRow::Buttons(vec![
                                    CreateButton::new("light_on")
                                        .label("Turn On")
                                        .style(ButtonStyle::Success),
                                    CreateButton::new("light_off")
                                        .label("Turn Off")
                                        .style(ButtonStyle::Danger),
                                ])]),
                        )
                        .await
                    {
                        error!("Error sending control message: {:?}", why);
                    }
                }
                Err(why) => error!("Error creating control channel: {:?}", why),
            }
        }
    }

    async fn execute_light_command(&self, command: &str) -> Result<(), String> {
        let output = Command::new("uv")
            .arg("run")
            .arg("kasa")
            .current_dir(&self.kasa_dir)
            .arg("--host")
            .arg(&self.kasa_device_ip)
            .arg("--username")
            .arg(&self.kasa_username)
            .arg("--password")
            .arg(&self.kasa_password)
            .arg(command)
            .output()
            .map_err(|e| format!("Failed to execute kasa command: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }

    async fn start_scheduler(&self) -> Result<(), Box<dyn std::error::Error>> {
        let scheduler = JobScheduler::new().await?;
        let handler = self.clone();

        // Log current time in different timezones
        let now = Utc::now();
        let local = Local::now();
        let toronto = now.with_timezone(&Toronto);

        info!("Current time - UTC: {}", now);
        info!("Current time - Local: {}", local);
        info!("Current time - Toronto: {}", toronto);

        // Turn off lights at midnight
        scheduler
            .add(Job::new_async("0 0 0 * * *", move |_, _| {
                let handler = handler.clone();
                Box::pin(async move {
                    info!("Running midnight job at {}", Local::now());
                    if let Err(e) = handler.execute_light_command("off").await {
                        error!("Failed to execute midnight light off command: {}", e);
                    } else {
                        info!("Successfully turned off light at midnight");
                    }
                })
            })?)
            .await?;

        // Turn on lights at 5 PM (17:00)
        let handler = self.clone();
        scheduler
            .add(Job::new_async("0 0 17 * * *", move |_, _| {
                let handler = handler.clone();
                Box::pin(async move {
                    info!("Running 5 PM job at {}", Local::now());
                    if let Err(e) = handler.execute_light_command("on").await {
                        error!("Failed to execute 5 PM light on command: {}", e);
                    } else {
                        info!("Successfully turned on light at 5 PM");
                    }
                })
            })?)
            .await?;

        // Start the scheduler
        scheduler.start().await?;

        Ok(())
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Component(component) = interaction {
            let content = match component.data.custom_id.as_str() {
                "light_on" => match self.execute_light_command("on").await {
                    Ok(_) => "Light turned on!",
                    Err(e) => {
                        error!("Error turning light on: {}", e);
                        "Failed to turn on light"
                    }
                },
                "light_off" => match self.execute_light_command("off").await {
                    Ok(_) => "Light turned off!",
                    Err(e) => {
                        error!("Error turning light off: {}", e);
                        "Failed to turn off light"
                    }
                },
                _ => "Unknown button",
            };

            if let Err(why) = component
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content(content)
                            .ephemeral(true),
                    ),
                )
                .await
            {
                error!("Cannot respond to button: {}", why);
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
        self.setup_control_channel(&ctx).await;
        if let Err(e) = self.start_scheduler().await {
            error!("Failed to start scheduler: {}", e);
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let token = get_env_var("DISCORD_TOKEN");

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILDS;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler::new())
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}
