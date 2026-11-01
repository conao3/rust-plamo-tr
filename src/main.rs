mod api;
mod lang;

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};

use crate::api::{Auth, Translator};
use crate::lang::{LANGUAGES, parse_language, resolve};

/// Translate text with PLaMo Translate through the browser extension API.
///
/// Without --source the source language is detected automatically. Without
/// --target, Japanese text is translated into English and anything else into
/// Japanese.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Text to translate. Read from stdin when omitted.
    text: Vec<String>,

    /// Source language code (auto-detected when omitted).
    #[arg(short, long, value_parser = parse_language)]
    source: Option<&'static str>,

    /// Target language code (en for Japanese input, ja otherwise).
    #[arg(short, long, value_parser = parse_language)]
    target: Option<&'static str>,

    /// Writing style of the translation.
    #[arg(long, value_enum)]
    style: Option<Style>,

    /// JSON file with the apiKey and refreshToken of a PLaMo Translate login.
    /// Defaults to plamo-tr/auth.json in the user config directory.
    #[arg(long, env = "PLAMO_AUTH_FILE")]
    auth_file: Option<PathBuf>,

    /// Print the supported language codes and exit.
    #[arg(long)]
    list_languages: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Style {
    Plain,
    Polite,
}

impl Style {
    fn as_str(self) -> &'static str {
        match self {
            Style::Plain => "plain",
            Style::Polite => "polite",
        }
    }
}

fn default_auth_file() -> Result<PathBuf> {
    let dir = dirs::config_dir().context("could not determine the user config directory")?;
    Ok(dir.join("plamo-tr").join("auth.json"))
}

fn read_input(args: &[String]) -> Result<String> {
    if !args.is_empty() {
        return Ok(args.join(" "));
    }
    let mut stdin = std::io::stdin();
    if stdin.is_terminal() {
        bail!("pass the text as arguments or through stdin");
    }
    let mut text = String::new();
    stdin.read_to_string(&mut text)?;
    Ok(text)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.list_languages {
        for language in LANGUAGES {
            println!("{}\t{}", language.code, language.name);
        }
        return Ok(());
    }

    let text = read_input(&cli.text)?;
    let text = text.trim();
    if text.is_empty() {
        bail!("nothing to translate");
    }
    let direction = resolve(cli.source, cli.target, text)?;
    let auth_file = match cli.auth_file {
        Some(path) => path,
        None => default_auth_file()?,
    };
    let translator = Translator::sign_in(&Auth::load(&auth_file)?)?;

    let mut stdout = std::io::stdout().lock();
    let mut ends_with_newline = false;
    translator.translate(text, &direction, cli.style.map(Style::as_str), |chunk| {
        if !chunk.is_empty() {
            ends_with_newline = chunk.ends_with('\n');
        }
        stdout.write_all(chunk.as_bytes())?;
        stdout.flush()?;
        Ok(())
    })?;
    if !ends_with_newline {
        writeln!(stdout)?;
    }
    Ok(())
}
