mod output;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, anyhow};
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use serde::{Serialize, Serializer};
use serde_json::{Value, json};

use plamo_tr::api::{Auth, Format, Options, ServiceError, Translator};
use plamo_tr::lang::{LANGUAGES, parse_language, resolve};

use output::OutputFormat;

const GLOBAL_OPTIONS_WITH_VALUE: &[&str] = &["--output"];

/// Translate text with PLaMo Translate through the browser extension API.
#[derive(Parser)]
#[command(version, disable_help_subcommand = true, subcommand_required = true)]
struct Cli {
    /// Format of the output.
    #[arg(long, global = true, help_heading = "Global Options", value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,

    /// Print the parameters and the full error chain to stderr.
    #[arg(long, global = true, help_heading = "Global Options")]
    debug: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Translate text.
    #[command(subcommand_required = true)]
    Text {
        #[command(subcommand)]
        command: TextCommand,
    },
    /// Languages supported by PLaMo Translate.
    #[command(subcommand_required = true)]
    Language {
        #[command(subcommand)]
        command: LanguageCommand,
    },
}

#[derive(Subcommand)]
enum TextCommand {
    /// Translate a text.
    ///
    /// Without --source-language-code the source language is detected
    /// automatically. Without --target-language-code, Japanese text is
    /// translated into English and anything else into Japanese. Either the
    /// source or the target must be ja or en.
    TranslateText(TranslateTextArgs),
}

#[derive(Subcommand)]
enum LanguageCommand {
    /// List the supported language codes.
    ListLanguages,
}

#[derive(Args)]
struct TranslateTextArgs {
    /// Text to translate. Use file:///dev/stdin to read standard input.
    #[arg(long, value_parser = parse_value)]
    text: Option<String>,

    /// Source language code (detected when omitted).
    #[arg(long, value_parser = parse_language_code)]
    source_language_code: Option<LanguageCode>,

    /// Target language code (en for Japanese text, ja otherwise).
    #[arg(long, value_parser = parse_language_code)]
    target_language_code: Option<LanguageCode>,

    /// Writing style of the translation.
    #[arg(long, value_enum)]
    writing_style: Option<WritingStyle>,

    /// JSON file with the apiKey and refreshToken of a PLaMo Translate login.
    /// Defaults to $PLAMO_AUTH_FILE, then plamo-tr/auth.json in the user
    /// config directory.
    #[arg(long)]
    auth_file: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
enum WritingStyle {
    Plain,
    Polite,
}

impl WritingStyle {
    fn as_str(self) -> &'static str {
        match self {
            WritingStyle::Plain => "plain",
            WritingStyle::Polite => "polite",
        }
    }
}

#[derive(Clone, Copy)]
struct LanguageCode(&'static str);

impl Serialize for LanguageCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

fn parse_language_code(value: &str) -> Result<LanguageCode, String> {
    parse_language(value).map(LanguageCode)
}

fn parse_value(value: &str) -> Result<String, String> {
    match value.strip_prefix("file://") {
        Some(path) => std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}")),
        None => Ok(value.to_owned()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct TranslateTextOutput {
    translated_text: String,
    source_language_code: Option<&'static str>,
    target_language_code: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ListLanguagesOutput {
    languages: Vec<LanguageOutput>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LanguageOutput {
    language_code: &'static str,
    language_name: &'static str,
}

enum Failure {
    Usage(anyhow::Error),
    Target(anyhow::Error),
    Other(anyhow::Error),
}

impl Failure {
    fn exit_code(&self) -> u8 {
        match self {
            Failure::Usage(_) => 252,
            Failure::Target(_) => 254,
            Failure::Other(_) => 255,
        }
    }

    fn error(&self) -> &anyhow::Error {
        match self {
            Failure::Usage(error) | Failure::Target(error) | Failure::Other(error) => error,
        }
    }
}

impl From<anyhow::Error> for Failure {
    fn from(error: anyhow::Error) -> Self {
        if error.downcast_ref::<ServiceError>().is_some() {
            Failure::Target(error)
        } else {
            Failure::Other(error)
        }
    }
}

fn usage(message: impl std::fmt::Display) -> Failure {
    Failure::Usage(anyhow!("{message}"))
}

/// Turn a trailing `help` word into `--help`, so that `help` works at every
/// level of the command tree.
fn rewrite_help(mut args: Vec<OsString>) -> Vec<OsString> {
    let mut index = 1;
    while index < args.len() {
        let arg = args[index].to_string_lossy();
        if GLOBAL_OPTIONS_WITH_VALUE.contains(&arg.as_ref()) {
            index += 2;
            continue;
        }
        if arg == "help" && index == args.len() - 1 {
            args[index] = "--help".into();
            break;
        }
        let is_global = arg == "--debug"
            || GLOBAL_OPTIONS_WITH_VALUE
                .iter()
                .any(|option| arg.starts_with(&format!("{option}=")));
        if arg.starts_with('-') && !is_global {
            break;
        }
        index += 1;
    }
    args
}

fn default_auth_file() -> anyhow::Result<PathBuf> {
    if let Some(path) = std::env::var_os("PLAMO_AUTH_FILE") {
        return Ok(path.into());
    }
    let dir = dirs::config_dir().context("could not determine the user config directory")?;
    Ok(dir.join("plamo-tr").join("auth.json"))
}

fn translate_text(args: TranslateTextArgs, debug: bool) -> Result<Value, Failure> {
    if debug {
        eprintln!(
            "parameters: {}",
            serde_json::to_string(&json!({
                "SourceLanguageCode": args.source_language_code,
                "TargetLanguageCode": args.target_language_code,
                "WritingStyle": args.writing_style,
                "AuthFile": args.auth_file,
            }))
            .unwrap_or_default()
        );
    }
    let Some(text) = args.text else {
        return Err(usage("the required parameter --text was not provided"));
    };
    let text = text.trim();
    if text.is_empty() {
        return Err(usage("--text is empty"));
    }
    let direction = resolve(
        args.source_language_code.map(|code| code.0),
        args.target_language_code.map(|code| code.0),
        text,
    )
    .map_err(Failure::Target)?;
    let auth_file = match args.auth_file {
        Some(path) => path,
        None => default_auth_file()?,
    };
    let translator = Translator::sign_in(&Auth::load(&auth_file)?)?;
    let options = Options {
        direction: &direction,
        format: Format::Plain,
        writing_style: args.writing_style.map(WritingStyle::as_str),
    };
    let translated_text = translator
        .translate_batch(&[text], &options)?
        .remove(0)
        .trim_end()
        .to_owned();
    to_value(TranslateTextOutput {
        translated_text,
        source_language_code: direction.source,
        target_language_code: direction.target,
    })
}

fn list_languages() -> ListLanguagesOutput {
    ListLanguagesOutput {
        languages: LANGUAGES
            .iter()
            .map(|language| LanguageOutput {
                language_code: language.code,
                language_name: language.name,
            })
            .collect(),
    }
}

fn to_value(output: impl Serialize) -> Result<Value, Failure> {
    serde_json::to_value(output).map_err(|error| Failure::Other(error.into()))
}

fn run(cli: Cli) -> Result<Value, Failure> {
    match cli.command {
        Command::Text {
            command: TextCommand::TranslateText(args),
        } => translate_text(args, cli.debug),
        Command::Language {
            command: LanguageCommand::ListLanguages,
        } => to_value(list_languages()),
    }
}

fn main() -> ExitCode {
    let args = rewrite_help(std::env::args_os().collect());
    let matches = match Cli::command().try_get_matches_from(args) {
        Ok(matches) => matches,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(if error.use_stderr() { 252 } else { 0 });
        }
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(252);
        }
    };
    let format = cli.output;
    let debug = cli.debug;
    let result = run(cli).map(|value| println!("{}", output::render(&value, format)));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            if debug {
                eprintln!("error: {:?}", failure.error());
            } else {
                eprintln!("error: {:#}", failure.error());
            }
            ExitCode::from(failure.exit_code())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    fn parse(words: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(rewrite_help(args(words)))
    }

    #[test]
    fn help_works_at_every_level() {
        for words in [
            &["plamo-tr", "help"][..],
            &["plamo-tr", "text", "help"],
            &[
                "plamo-tr",
                "--output",
                "text",
                "text",
                "translate-text",
                "help",
            ],
        ] {
            let error = parse(words).err().expect("help is not a command");
            assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
        }
    }

    #[test]
    fn help_as_a_parameter_value_is_kept() {
        let cli = parse(&["plamo-tr", "text", "translate-text", "--text", "help"]).unwrap();
        let Command::Text {
            command: TextCommand::TranslateText(args),
        } = cli.command
        else {
            panic!("parsed another command");
        };
        assert_eq!(args.text.as_deref(), Some("help"));
    }

    #[test]
    fn rejects_positional_arguments() {
        assert!(parse(&["plamo-tr", "text", "translate-text", "こんにちは"]).is_err());
    }

    #[test]
    fn reads_values_from_files() {
        let path = std::env::temp_dir().join("plamo-tr-parse-value-test.txt");
        std::fs::write(&path, "こんにちは").unwrap();
        assert_eq!(
            parse_value(&format!("file://{}", path.display())),
            Ok("こんにちは".to_owned())
        );
        std::fs::remove_file(&path).unwrap();
        assert_eq!(parse_value("Hello"), Ok("Hello".to_owned()));
    }

    #[test]
    fn service_errors_exit_with_254() {
        let failure = Failure::from(anyhow::Error::new(ServiceError("quota".into())));
        assert_eq!(failure.exit_code(), 254);
        assert_eq!(Failure::from(anyhow!("network")).exit_code(), 255);
    }

    #[test]
    fn languages_are_listed_with_pascal_case_keys() {
        let value = to_value(list_languages()).ok().unwrap();
        assert_eq!(
            value["Languages"][0],
            json!({"LanguageCode": "ja", "LanguageName": "Japanese"})
        );
    }
}
