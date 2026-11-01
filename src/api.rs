use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};

use crate::lang::Direction;

const API_URL: &str = "https://api.translate.preferredai.jp";
const TOKEN_URL: &str = "https://securetoken.googleapis.com/v1/token";
const SITE_ORIGIN: &str = "https://translate.preferredai.jp";
const ATTEMPTS: u32 = 4;

#[derive(Clone, Deserialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Plain,
    Html,
}

impl Format {
    fn as_str(self) -> &'static str {
        match self {
            Format::Plain => "plain",
            Format::Html => "html",
        }
    }
}

pub struct Options<'a> {
    pub direction: &'a Direction,
    pub format: Format,
    pub writing_style: Option<&'a str>,
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
    expires_in: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TranslateRequest<'a> {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    burst: bool,
    format: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_language: Option<&'a str>,
    source_text: &'a [&'a str],
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
    index: usize,
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

struct IdToken {
    value: String,
    expires_at: Instant,
}

pub struct Translator {
    client: Client,
    auth: Auth,
    id_token: Mutex<IdToken>,
}

impl Translator {
    pub fn sign_in(auth: &Auth) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()?;
        let id_token = refresh(&client, auth)?;
        Ok(Self {
            client,
            auth: auth.clone(),
            id_token: Mutex::new(id_token),
        })
    }

    fn id_token(&self) -> Result<String> {
        let mut id_token = self.id_token.lock().expect("id token lock poisoned");
        if Instant::now() + Duration::from_secs(60) >= id_token.expires_at {
            *id_token = refresh(&self.client, &self.auth)?;
        }
        Ok(id_token.value.clone())
    }

    fn send(&self, request: &TranslateRequest) -> Result<Response> {
        for attempt in 1..=ATTEMPTS {
            let sent = self
                .client
                .post(format!("{API_URL}/translate-stream"))
                .bearer_auth(self.id_token()?)
                .header("Accept", "text/event-stream")
                .header("X-App-Source", "browser-extension")
                .header("X-Plamo-User-Agent", "chrome_extension")
                .json(request)
                .send();
            let retryable = match sent {
                Ok(response) if response.status().is_server_error() && attempt < ATTEMPTS => {
                    format!("PLaMo Translate returned {}", response.status())
                }
                Ok(response) => return check_status(response),
                Err(error) if attempt < ATTEMPTS => error.to_string(),
                Err(error) => return Err(error).context("failed to reach PLaMo Translate"),
            };
            eprintln!("{retryable} (attempt {attempt} of {ATTEMPTS})");
            std::thread::sleep(Duration::from_secs(5 * u64::from(attempt)));
        }
        unreachable!()
    }

    /// Translate `texts` in one request, calling `on_chunk` with the index of
    /// the text and the next piece of its translation as they arrive.
    pub fn translate_stream(
        &self,
        texts: &[&str],
        options: &Options,
        mut on_chunk: impl FnMut(usize, &str) -> Result<()>,
    ) -> Result<()> {
        let request = TranslateRequest {
            burst: texts.len() > 1,
            format: options.format.as_str(),
            source_language: options.direction.source,
            source_text: texts,
            stream: true,
            target_language: options.direction.target,
            writing_style: options.writing_style,
        };
        for line in BufReader::new(self.send(&request)?).lines() {
            for chunk in parse_event(&line?)? {
                if chunk.index >= texts.len() {
                    bail!("PLaMo Translate answered for text {}", chunk.index);
                }
                on_chunk(chunk.index, &chunk.text)?;
            }
        }
        Ok(())
    }

    /// Translate `texts` in one request and return the translations in order.
    pub fn translate_batch(&self, texts: &[&str], options: &Options) -> Result<Vec<String>> {
        let mut translations = vec![String::new(); texts.len()];
        self.translate_stream(texts, options, |index, text| {
            translations[index].push_str(text);
            Ok(())
        })?;
        Ok(translations)
    }
}

fn refresh(client: &Client, auth: &Auth) -> Result<IdToken> {
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
    let expires_in: u64 = token
        .expires_in
        .parse()
        .context("unexpected expires_in from the token endpoint")?;
    Ok(IdToken {
        value: token.id_token,
        expires_at: Instant::now() + Duration::from_secs(expires_in),
    })
}

fn check_status(response: Response) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().unwrap_or_default();
    match serde_json::from_str::<ErrorBody>(&body) {
        Ok(ErrorBody { error }) => bail!("{} ({status}): {}", error.code, error.message),
        Err(_) => bail!("PLaMo Translate returned {status}: {body}"),
    }
}

fn parse_event(line: &str) -> Result<Vec<Chunk>> {
    let Some(data) = line.strip_prefix("data: ") else {
        return Ok(Vec::new());
    };
    if data == "[DONE]" {
        return Ok(Vec::new());
    }
    let event: Event = serde_json::from_str(data)
        .with_context(|| format!("unexpected event from PLaMo Translate: {data}"))?;
    Ok(event.results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stream_events() {
        let line = r#"data: {"results":[{"index":1,"text":"The weather","completed":false}]}"#;
        let chunks = parse_event(line).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(
            (chunks[0].index, chunks[0].text.as_str()),
            (1, "The weather")
        );
        assert!(parse_event("data: [DONE]").unwrap().is_empty());
        assert!(parse_event("").unwrap().is_empty());
    }

    #[test]
    fn single_text_request_omits_burst_and_auto_source() {
        let request = TranslateRequest {
            burst: false,
            format: "plain",
            source_language: None,
            source_text: &["こんにちは"],
            stream: true,
            target_language: "en",
            writing_style: None,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"format":"plain","sourceText":["こんにちは"],"stream":true,"targetLanguage":"en"}"#
        );
    }

    #[test]
    fn batch_request_sends_burst() {
        let request = TranslateRequest {
            burst: true,
            format: "html",
            source_language: Some("en"),
            source_text: &["Fonts", "Faces"],
            stream: true,
            target_language: "ja",
            writing_style: None,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"burst":true,"format":"html","sourceLanguage":"en","sourceText":["Fonts","Faces"],"stream":true,"targetLanguage":"ja"}"#
        );
    }
}
