# plamo-tr

A command-line client for [PLaMo Translate](https://translate.preferredai.jp/) that talks to the same API as the PLaMo Translate browser extension.

This is an unofficial tool and is not affiliated with Preferred Networks.

## Usage

```console
$ plamo-tr 今日はいい天気ですね。
The weather is nice today, isn't it?

$ echo "The weather is nice today, so let us go for a walk." | plamo-tr
今日はいい天気だから、散歩に行こう。

$ plamo-tr -t ko "The weather is nice today, so let us go for a walk."
오늘 날씨가 좋으니 산책하러 가요.

$ plamo-tr -s ja -t zh-Hans 今日はいい天気なので散歩に行きましょう。
今天天气很好，我们去散散步吧。
```

The text comes from the arguments, or from stdin when no argument is given. The translation is streamed to stdout.

| Option | Meaning |
|---|---|
| `-s`, `--source <LANG>` | Source language. Detected automatically when omitted. |
| `-t`, `--target <LANG>` | Target language. When omitted, Japanese text is translated into English and anything else into Japanese. |
| `--style <plain\|polite>` | Writing style of the translation. |
| `--auth-file <PATH>` | Login file (see below). Also read from `PLAMO_AUTH_FILE`. |
| `--list-languages` | Print the supported language codes. |

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

## Install

```console
$ nix run github:conao3/rust-plamo-tr -- 今日はいい天気ですね。
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
