# AGENTS.md

`plamo-tr` is a Rust CLI that translates text through the PLaMo Translate browser extension API (`https://api.translate.preferredai.jp/translate-stream` with `X-App-Source: browser-extension`).

## Layout

- `src/lib.rs`: the library crate (`plamo_tr::api` and `plamo_tr::lang`), used by the CLI and by other tools.
- `src/main.rs`: command-line arguments, input, and streamed output.
- `src/lang.rs`: supported languages, local detection with whatlang, and the rules that pick the source and target.
- `src/api.rs`: Firebase token refresh from the auth file (renewed before the ID token expires), the streaming translate request for one or more texts in plain text or HTML, and retries on network and server errors.

## Conventions

- Language codes and names follow the language list of the PLaMo Translate browser extension.
- Either the source or the target must be `ja` or `en`. An omitted source is left out of the request when the target is `ja` or `en`, and is detected locally otherwise.
- Commit messages, comments, and docs are in English.

## Verification

Run everything inside the devShell:

```console
$ nix develop --command make check
$ nix build
```

The tests do not call the API. To try a real translation, point `PLAMO_AUTH_FILE` at an auth file (README "Login") and run `cargo run -- <text>`.
