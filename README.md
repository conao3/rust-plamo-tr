# plamo-tr

A command-line client for [PLaMo Translate](https://translate.preferredai.jp/) that talks to the same API as the PLaMo Translate browser extension.

This is an unofficial tool and is not affiliated with Preferred Networks.

## Usage

```console
$ plamo-tr text translate-text --text 今日はいい天気ですね。
{
  "TranslatedText": "The weather is nice today, isn't it?",
  "SourceLanguageCode": null,
  "TargetLanguageCode": "en"
}

$ echo "The weather is nice today, so let us go for a walk." \
    | plamo-tr text translate-text --text file:///dev/stdin --query TranslatedText --output text
今日はいい天気だから、散歩に行こう。

$ plamo-tr --output text --query TranslatedText text translate-text \
    --target-language-code ko --text "The weather is nice today, so let us go for a walk."
오늘 날씨가 좋으니 산책하러 가요.

$ plamo-tr text translate-text --source-language-code ja --target-language-code zh-Hans \
    --text 今日はいい天気なので散歩に行きましょう。 --query TranslatedText --output text
今天天气很好，我们去散散步吧。

$ plamo-tr language list-languages --output table --query 'Languages[:2]'
+--------------+--------------+
| LanguageCode | LanguageName |
+--------------+--------------+
| ja           | Japanese     |
| en           | English      |
+--------------+--------------+
```

The command line follows the grammar of the AWS CLI: `plamo-tr [global options] <command> <subcommand> [--parameter value ...]`. Every parameter is named; there are no positional arguments.

| Command | Subcommand | Output |
|---|---|---|
| `text` | `translate-text` | `TranslatedText`, `SourceLanguageCode` (`null` when PLaMo Translate detects it), `TargetLanguageCode` |
| `language` | `list-languages` | `Languages`, a list of `LanguageCode` and `LanguageName` |

Parameters of `text translate-text`:

| Parameter | Meaning |
|---|---|
| `--text <TEXT>` | Text to translate. Required. `file:///dev/stdin` reads standard input. |
| `--source-language-code <LANG>` | Source language. Detected automatically when omitted. |
| `--target-language-code <LANG>` | Target language. When omitted, Japanese text is translated into English and anything else into Japanese. |
| `--writing-style <plain\|polite>` | Writing style of the translation. |
| `--auth-file <PATH>` | Login file (see below). Defaults to `PLAMO_AUTH_FILE`, then the user config directory. |

Every subcommand also takes:

- `--cli-input-json <JSON>`: all parameters as one JSON object whose keys are the parameter names in PascalCase (`Text`, `SourceLanguageCode`, ...). Parameters given on the command line take precedence.
- `--generate-cli-skeleton [input|output]`: print a JSON skeleton of the input (default) or the output and exit.

A value that starts with `file://` is read from that file, for example `--text file://article.txt` or `--cli-input-json file://input.json`.

Global options, accepted anywhere on the command line:

| Option | Meaning |
|---|---|
| `--output <json\|text\|table>` | Output format. Defaults to `json`. `text` prints the values of an object tab-separated on one line and each list item on its own line. |
| `--query <JMESPath>` | Select part of the output with a [JMESPath](https://jmespath.org/) expression, for example `TranslatedText`. |
| `--debug` | Print the parameters and the full error chain to stderr. |
| `--version` | Print the version. |

`help` prints the help of each level: `plamo-tr help`, `plamo-tr text help`, `plamo-tr text translate-text help`.

| Exit code | Meaning |
|---|---|
| 0 | Success. |
| 252 | The command line or a parameter is invalid (unknown command or parameter, unsupported language code, missing or empty `--text`, invalid `--cli-input-json` or `--query`). |
| 254 | PLaMo Translate cannot handle the request: it answered with an error (including a rejected login), or the language pair is not supported. |
| 255 | Any other failure, such as an unreadable auth file or an unreachable network. |

PLaMo Translate translates between Japanese or English and the other supported languages, so either the source or the target must be `ja` or `en`. When the source is omitted and the target is `ja` or `en`, PLaMo Translate detects the source language itself. For other targets, `plamo-tr` detects the source language locally with [whatlang](https://github.com/greyblake/whatlang-rs) and sends it with the request.

## Login

`plamo-tr` uses the login of the PLaMo Translate web app. Sign in at <https://translate.preferredai.jp/>, open the developer tools console on that page, and run:

```js
await new Promise((resolve) => {
  indexedDB.open("firebaseLocalStorageDb").onsuccess = (event) => {
    const store = event.target.result
      .transaction("firebaseLocalStorage")
      .objectStore("firebaseLocalStorage");
    store.getAll().onsuccess = (rows) => {
      const user = rows.target.result.find((row) => row.value?.stsTokenManager)?.value;
      resolve(JSON.stringify({ apiKey: user.apiKey, refreshToken: user.stsTokenManager.refreshToken }));
    };
  };
});
```

Save the printed JSON as `plamo-tr/auth.json` in the user config directory (`~/.config/plamo-tr/auth.json` on Linux, `~/Library/Application Support/plamo-tr/auth.json` on macOS) and make it readable only by you:

```console
$ mkdir -p ~/.config/plamo-tr
$ install -m 600 /dev/stdin ~/.config/plamo-tr/auth.json <<'EOF'
{"apiKey":"...","refreshToken":"..."}
EOF
```

The refresh token is a long-lived credential for your PLaMo account. Do not share it.

Translations count toward the browser extension quota of your plan (unlimited on the Lite plan), not the text translation quota.

## Library

The crate also builds as a library (`plamo_tr`) for tools that translate many texts. `Translator::translate_batch` sends several texts in one request, in plain text or HTML (tags are kept), and the ID token is renewed while a long job runs.

```toml
[dependencies]
plamo-tr = { git = "https://github.com/conao3/rust-plamo-tr" }
```

```rust
use plamo_tr::api::{Auth, Format, Options, Translator};
use plamo_tr::lang::Direction;

let translator = Translator::sign_in(&Auth::load("auth.json".as_ref())?)?;
let direction = Direction { source: Some("en"), target: "ja" };
let options = Options { direction: &direction, format: Format::Html, writing_style: None };
let translations = translator.translate_batch(&["<code>C-x C-f</code> visits a file.", "Fonts"], &options)?;
```

## Install

```console
$ nix run github:conao3/rust-plamo-tr -- text translate-text --text 今日はいい天気ですね。
$ nix profile install github:conao3/rust-plamo-tr
```

Or build with Cargo:

```console
$ cargo install --git https://github.com/conao3/rust-plamo-tr
```

## Development

```console
$ nix develop
$ make check
```

`make check` runs `nix fmt -- --ci`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.

## License

Apache-2.0
