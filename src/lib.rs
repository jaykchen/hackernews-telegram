use flowsnet_platform_sdk::logger;
use openai_flows::{
    chat::{self, ChatModel, ChatOptions},
    OpenAIFlows,
};
use serde_json::json;
use store_flows::{get, set};
use tg_flows::{listen_to_update, Method, Telegram, Update, UpdateKind};
use tokio::time::sleep;
use web_scraper_flows::get_page_text;
use std::time::{SystemTime, UNIX_EPOCH};
use http_req::{request, request::Request, uri::Uri};
use serde::{Deserialize, Serialize};


#[no_mangle]
#[tokio::main(flavor = "current_thread")]
pub async fn run() -> anyhow::Result<()> {
    logger::init();
    let telegram_token = std::env::var("telegram_token").expect("Missing telegram_token");
    let placeholder_text = std::env::var("placeholder").unwrap_or("Typing ...".to_string());
    let help_mesg = std::env::var("help_mesg").unwrap_or("You can enter text or upload an image with text to chat with this bot. The bot can take several different assistant roles. Type command /qa or /translate or /summarize or /code or /reply_tweet to start.".to_string());

    let mut chat_id = 0;
    match get_chat_id(telegram_token.clone(), "username".to_string()) {
        Ok(id) => chat_id = id,
        Err(_e) => log::debug!("Unable to retrieve chat_id {:}?", _e.to_string(),),
    }
    // Spawn a new task to run every hour

    let tg_token_clone = telegram_token.clone();
    tokio::spawn(async move {
        loop {
            // Assume that get_news_updates() returns a vector of updates as strings.
            match get_news_updates().await {
                Ok(news_updates) => {
                    send_news_update(tg_token_clone.clone(), chat_id, &news_updates[0]).await;
                }
                Err(e) => {
                    log::error!("Error getting news updates: {:?}", e);
                }
            }
    
            // Sleep for an hour
            tokio::time::sleep(std::time::Duration::from_secs(60 * 60)).await;
        }
    })
    .await;
    

    listen_to_update(&telegram_token.clone(), |_| async {}).await;

    Ok(())
}

async fn send_news_update(telegram_token: String, chat_id: i64, update: &str) {
    let tele = Telegram::new(telegram_token);
    let _ = tele.send_message(tg_flows::ChatId(chat_id), update);
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

async fn get_news_updates() -> anyhow::Result<Vec<String>> {
    let keyword = std::env::var("KEYWORD").unwrap_or("ChatGPT".to_string());
    let now = SystemTime::now();
    let dura = now.duration_since(UNIX_EPOCH).unwrap().as_secs() - 3600;
    let url = format!("https://hn.algolia.com/api/v1/search_by_date?tags=story&query={keyword}&numericFilters=created_at_i>{dura}");

    let mut writer = Vec::new();
    let _ = request::get(url, &mut writer)?;
    let search: Search = serde_json::from_slice::<Search>(&writer)?;
    let mut res = Vec::new();
    for hit in search.hits {
        let title = &hit.title;
        let author = &hit.author;
        let post = format!("https://news.ycombinator.com/item?id={}", &hit.object_id);
        let mut inner_url = "".to_string();

        let _text = match &hit.url {
            Some(u) => {
                inner_url = u.clone();
                get_page_text(u)
                    .await
                    .unwrap_or("failed to scrape text with hit url".to_string())
            }
            None => get_page_text(&post)
                .await
                .unwrap_or("failed to scrape text with post url".to_string()),
        };

        let summary = if _text.split_whitespace().count() > 100 {
            get_summary_truncated(&_text).await?
        } else {
            format!("Bot found minimal info on webpage to warrant a summary, please see the text on the page the Bot grabbed below if there are any, or use the link above to see the news at its source:\n{_text}")
        };

        let source = if !inner_url.is_empty() {
            format!("<{inner_url}|source>")
        } else {
            "".to_string()
        };

        let msg = format!("- <{post}|*{title}*>\n{source} by {author}\n{summary}");
        res.push(msg);
    }
    Ok(res)
}

#[derive(Deserialize)]
pub struct Search {
    pub hits: Vec<Hit>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Hit {
    pub title: String,
    pub url: Option<String>,
    #[serde(rename = "objectID")]
    pub object_id: String,
    pub author: String,
    pub created_at_i: i64,
}

async fn get_summary_truncated(inp: &str) -> anyhow::Result<String> {
    let mut openai = OpenAIFlows::new();
    openai.set_retry_times(3);

    let news_body = inp
        .split_whitespace()
        .take(10000)
        .collect::<Vec<&str>>()
        .join(" ");

    let chat_id = format!("summary#99");
    let system = &format!("You're an AI assistant.");

    let co = ChatOptions {
        model: ChatModel::GPT35Turbo16K,
        restart: true,
        system_prompt: Some(system),
        max_tokens: Some(128),
        temperature: Some(0.8),
        ..Default::default()
    };

    let question = format!("summarize this within 100 words: {news_body}");

    match openai.chat_completion(&chat_id, &question, &co).await {
        Ok(r) => Ok(r.choice),
        Err(_e) => Err(anyhow::Error::msg(_e.to_string())),
    }
}
