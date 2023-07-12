use flowsnet_platform_sdk::logger;
use openai_flows::{
    chat::{self, ChatModel, ChatOptions},
    OpenAIFlows,
};
use serde_json::json;
use store_flows::{get, set};
use tg_flows::{listen_to_update, Method, Telegram, Update, UpdateKind};
use tokio::time::sleep;

#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn run() -> anyhow::Result<()> {
    logger::init();
    let telegram_token = std::env::var("telegram_token").unwrap();
    let placeholder_text = std::env::var("placeholder").unwrap_or("Typing ...".to_string());
    let help_mesg = std::env::var("help_mesg").unwrap_or("You can enter text or upload an image with text to chat with this bot. The bot can take several different assistant roles. Type command /qa or /translate or /summarize or /code or /reply_tweet to start.".to_string());

    let tg_token_clone = telegram_token.clone();

    let mut chat_id = 0;
    match get_chat_id(tg_token_clone, "username".to_string()) {
        Ok(id) => chat_id = id,
        Err(_e) => log::debug!("Unable to retrieve chat_id {:}?", _e.to_string(),),
    }
    // Spawn a new task to run every hour
    tokio::spawn(async move {
        loop {
            // Assume that get_news_updates() returns a vector of updates as strings.
            let news_updates = get_news_updates().await;

            send_news_update(telegram_token.clone(), chat_id, &news_updates).await;

            // Sleep for an hour
            tokio::time::sleep(std::time::Duration::from_secs(60 * 60)).await;
        }
    })
    .await;

    listen_to_update(&telegram_token, |_| async {}).await;

    Ok(())
}

async fn send_news_update(telegram_token: String, chat_id: i64, update: &str) {
    let tele = Telegram::new(telegram_token);
    let _ = tele.send_message(tg_flows::ChatId(chat_id), update);
}
async fn get_news_updates() -> String {
    String::new()
}

use serde_json::Value;

fn get_chat_id(token: String, username: String) -> Result<i64, Box<dyn std::error::Error>> {
    let telegram = Telegram::new(token);
    let method = Method::GetChat;
    let body = json!({ "chat_id": format!("@{}", username) });

    let result: Value = telegram.request(method, body.to_string().as_bytes())?;
    if let Some(chat_id) = result.get("id") {
        return Ok(chat_id.as_i64().ok_or("Unable to retrieve chat_id")?);
    }

    Err("Unable to retrieve chat_id".into())
}
