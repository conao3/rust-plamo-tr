# AGENTS.md

`plamo-tr` is a Rust CLI that translates text through the PLaMo Translate browser extension API (`https://api.translate.preferredai.jp/translate-stream` with `X-App-Source: browser-extension`).

## Layout

- `src/lib.rs`: the library crate (`plamo_tr::api` and `plamo_tr::lang`), used by the CLI and by other tools.
- `src/main.rs`: the command table (clap derive: `text translate-text`, `language list-languages`, global options), `file://` values, and exit codes.
- `src/output.rs`: the `json`, `text`, and `table` output formats.
- `src/lang.rs`: supported languages, local detection with whatlang, and the rules that pick the source and target.
- `src/api.rs`: Firebase token refresh from the auth file (renewed before the ID token expires), the streaming translate request for one or more texts in plain text or HTML, and retries on network and server errors.

## Conventions

- Language codes and names follow the language list of the PLaMo Translate browser extension.
- Either the source or the target must be `ja` or `en`. An omitted source is left out of the request when the target is `ja` or `en`, and is detected locally otherwise.
- The command line follows the AWS CLI grammar: `<command> <subcommand>` with named `--kebab-case` parameters only, PascalCase keys in the output, and exit codes 0 / 252 (command line or parameter) / 254 (PLaMo Translate error or unsupported language pair) / 255 (other). A new parameter is a field of the subcommand's `Args` struct; its help follows from that field.
- Commit messages, comments, and docs are in English.

## Verification

Run everything inside the devShell:

```console
$ nix develop --command make check
$ nix build
```

The tests do not call the API. To try a real translation, point `PLAMO_AUTH_FILE` at an auth file (README "Login") and run `cargo run -- text translate-text --text <text>`.
