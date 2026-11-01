use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::lang::Direction;

const API_URL: &str = "https://api.translate.preferredai.jp";
const TOKEN_URL: &str = "https://securetoken.googleapis.com/v1/token";
const SITE_ORIGIN: &str = "https://translate.preferredai.jp";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Auth {
    api_key: String,
    refresh_token: String,
}

impl Auth {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read the auth file {}", path.display()))?;
        serde_json::from_str(&content).with_context(|| {
            format!(
                "{} must be a JSON object with apiKey and refreshToken",
                path.display()
            )
        })
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TranslateRequest<'a> {
    format: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_language: Option<&'a str>,
    source_text: [&'a str; 1],
    stream: bool,
    target_language: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    writing_style: Option<&'a str>,
}

#[derive(Deserialize)]
struct Event {
    results: Vec<Chunk>,
}

#[derive(Deserialize)]
struct Chunk {
    text: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    code: String,
    message: String,
}

pub struct Translator {
    client: Client,
    id_token: String,
}

impl Translator {
    pub fn sign_in(auth: &Auth) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()?;
        let response = client
            .post(TOKEN_URL)
            .query(&[("key", &auth.api_key)])
            .header("origin", SITE_ORIGIN)
            .header("referer", format!("{SITE_ORIGIN}/"))
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", &auth.refresh_token),
            ])
            .send()
            .context("failed to reach the token endpoint")?;
        if !response.status().is_success() {
            bail!(
                "token refresh failed ({}): sign in to {SITE_ORIGIN} again and update the auth file",
                response.status()
            );
        }
        let token: TokenResponse = response.json()?;
        Ok(Self {
            client,
            id_token: token.id_token,
        })
    }

    pub fn translate(
        &self,
        text: &str,
        direction: &Direction,
        writing_style: Option<&str>,
        mut on_text: impl FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        let request = TranslateRequest {
            format: "plain",
            source_language: direction.source,
            source_text: [text],
            stream: true,
            target_language: direction.target,
            writing_style,
        };
        let response = self
            .client
            .post(format!("{API_URL}/translate-stream"))
            .bearer_auth(&self.id_token)
            .header("Accept", "text/event-stream")
            .header("X-App-Source", "browser-extension")
            .header("X-Plamo-User-Agent", "chrome_extension")
            .json(&request)
            .send()
            .context("failed to reach PLaMo Translate")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            match serde_json::from_str::<ErrorBody>(&body) {
                Ok(ErrorBody { error }) => bail!("{} ({status}): {}", error.code, error.message),
                Err(_) => bail!("PLaMo Translate returned {status}: {body}"),
            }
        }
        for line in BufReader::new(response).lines() {
            if let Some(text) = parse_event(&line?)? {
                on_text(&text)?;
            }
        }
        Ok(())
    }
}

fn parse_event(line: &str) -> Result<Option<String>> {
    let Some(data) = line.strip_prefix("data: ") else {
        return Ok(None);
    };
    if data == "[DONE]" {
        return Ok(None);
    }
    let event: Event = serde_json::from_str(data)
        .with_context(|| format!("unexpected event from PLaMo Translate: {data}"))?;
    Ok(Some(
        event.results.into_iter().map(|chunk| chunk.text).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stream_events() {
        let line = r#"data: {"results":[{"index":0,"text":"The weather","completed":false}]}"#;
        assert_eq!(parse_event(line).unwrap().as_deref(), Some("The weather"));
        assert_eq!(parse_event("data: [DONE]").unwrap(), None);
        assert_eq!(parse_event("").unwrap(), None);
    }

    #[test]
    fn omits_auto_source_from_request() {
        let request = TranslateRequest {
            format: "plain",
            source_language: None,
            source_text: ["こんにちは"],
            stream: true,
            target_language: "en",
            writing_style: None,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"format":"plain","sourceText":["こんにちは"],"stream":true,"targetLanguage":"en"}"#
        );
    }
}
