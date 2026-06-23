use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;

use crate::store;

#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    role: String,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RequestBody {
    #[serde(default)]
    messages: Vec<Message>,
}

#[derive(Debug, Deserialize)]
struct ResponseBody {
    #[serde(default)]
    choices: Vec<ResponseChoice>,
}

#[derive(Debug, Deserialize)]
struct ResponseChoice {
    message: Option<Message>,
}

#[derive(Debug, Deserialize)]
struct Transaction {
    #[allow(dead_code)]
    id: String,
    request: Option<RequestData>,
    response: Option<ResponseData>,
}

#[derive(Debug, Deserialize)]
struct RequestData {
    body: Option<RequestBody>,
}

#[derive(Debug, Deserialize)]
struct ResponseData {
    body: Option<ResponseBody>,
}

fn sanitize_content(content: &str) -> String {
    // Replace escaped \n with actual newlines (matches JS: content.replace(/\\n/g, '\n'))
    content.replace("\\n", "\n")
}

fn format_role(role: &str) -> String {
    match role {
        "system" => "System".to_string(),
        "user" => "User".to_string(),
        "assistant" => "Assistant".to_string(),
        _ => {
            if role.is_empty() {
                "Unknown".to_string()
            } else {
                let mut chars = role.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                }
            }
        }
    }
}

fn conversation_to_markdown(messages: &[Message]) -> String {
    let mut md = String::new();
    for msg in messages {
        let role = format_role(&msg.role);
        let content = sanitize_content(msg.content.as_deref().unwrap_or(""));
        md.push_str(&format!("=== {role} ===\n{content}\n\n"));
    }
    md.trim().to_string()
}

fn response_to_markdown(content: &str) -> String {
    let sanitized = sanitize_content(content);
    format!("=== Assistant ===\n{sanitized}")
}

pub async fn download_conversation(Path(id): Path<String>) -> Response<String> {
    let uuid = match store::validate_id(&id) {
        Ok(u) => u,
        Err(e) => {
            return Response::builder()
                .status(e)
                .body("Invalid transaction id".to_string())
                .expect("static response builder cannot fail");
        }
    };
    let tx = match store::load_tx_typed::<Transaction>(&uuid).await {
        Ok(t) => t,
        Err(e) => {
            return Response::builder()
                .status(e)
                .body("Transaction not found".to_string())
                .expect("static response builder cannot fail");
        }
    };

    let messages: Vec<Message> = tx.request
        .and_then(|r| r.body)
        .map(|b| b.messages)
        .unwrap_or_default();

    if messages.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("No messages to download".to_string())
            .expect("static response builder cannot fail");
    }

    let md = conversation_to_markdown(&messages);
    let filename = format!("conversation_{uuid}.md");

    Response::builder()
        .header("Content-Type", "text/markdown")
        .header("Content-Disposition", format!("attachment; filename=\"{filename}\""))
        .body(md)
        .expect("static response builder cannot fail")
}

pub async fn download_response(Path(id): Path<String>) -> Response<String> {
    let uuid = match store::validate_id(&id) {
        Ok(u) => u,
        Err(e) => {
            return Response::builder()
                .status(e)
                .body("Invalid transaction id".to_string())
                .expect("static response builder cannot fail");
        }
    };
    let tx = match store::load_tx_typed::<Transaction>(&uuid).await {
        Ok(t) => t,
        Err(e) => {
            return Response::builder()
                .status(e)
                .body("Transaction not found".to_string())
                .expect("static response builder cannot fail");
        }
    };

    let choices: Vec<ResponseChoice> = tx.response
        .and_then(|r| r.body)
        .map(|b| b.choices)
        .unwrap_or_default();

    if choices.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("No response to download".to_string())
            .expect("static response builder cannot fail");
    }

    let content = choices
        .first()
        .and_then(|c| c.message.as_ref())
        .and_then(|m| m.content.as_deref())
        .unwrap_or("");

    if content.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body("Response has no content".to_string())
            .expect("static response builder cannot fail");
    }

    let md = response_to_markdown(content);
    let filename = format!("response_{uuid}.md");

    Response::builder()
        .header("Content-Type", "text/markdown")
        .header("Content-Disposition", format!("attachment; filename=\"{filename}\""))
        .body(md)
        .expect("static response builder cannot fail")
}
