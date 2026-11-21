# Doze

A fast, user-friendly **REST, WebSocket, and RPC (JSON-RPC 2.0 & gRPC)** desktop client written in Rust using the [Dioxus](https://dioxuslabs.com/) framework.

## Key Features

- **Multi-Protocol Testing**
  - **REST (HTTP/HTTPS)**: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`, query params, `:path` params, custom headers, Auth (Bearer, Basic, API Key in Header/Query), Bodies (JSON with pretty formatting, Raw, Form URL-Encoded, Multipart with file uploads, Binary), per-request timeout/redirect/TLS overrides, response inspector (Pretty/Raw/Headers/Cookies, search, copy, save), plus **cURL Export & Import**.
  - **WebSocket (WS/WSS)**: Live connect/disconnect lifecycle, custom handshake headers, query params, subprotocols, auth, Text/JSON & Binary (hex) frame composer, saved message presets, ping frames, and a real-time filterable message stream log.
  - **RPC (JSON-RPC 2.0 & gRPC)**:
    - **JSON-RPC 2.0** over HTTP/S with method, ID, JSON params, and structured `result`/`error` envelope inspection.
    - **Dynamic gRPC** powered by pure-Rust `protox` + `prost-reflect` + `tonic` (no external `protoc` binary required): select any `.proto` file, browse services & methods, auto-generate sample JSON payloads, send metadata/auth headers, and inspect JSON-decoded protobuf responses & trailers.

- **Source-Control Friendly Project `.doze` Package**
  - Every project stores its collections, manifest, and non-secret environments as clean, diff-friendly TOML files inside `<project>/.doze/`:
    ```text
    .doze/
    ├── .gitignore             # Safety guard ignoring local/ and *.secrets.toml
    ├── doze.toml             # Project manifest (name, project_id, default headers, proto include dirs)
    ├── environments/
    │   └── dev.toml           # Shared non-secret variables + declared [[secret_keys]] (NAMES ONLY)
    └── requests/
        └── **/*.toml          # Individual request & folder.toml files for clean Git diffs
    ```
  - **Zero Secrets in Source Control**: When you declare a secret in an environment, only its key name & optional team hint are written into `.doze/environments/<env>.toml`. The actual secret value is stored **exclusively** in your OS user directory (`<user_config_dir>/secrets/<project_id>.toml`) and is never written into `.doze/`.

- **User-Level Settings**
  - Stored in your OS-standard user configuration directory (via `directories`):
    - Windows: `%APPDATA%\doze\Doze\config\settings.toml`
    - macOS: `~/Library/Application Support/dev.doze.Doze/settings.toml`
    - Linux: `~/.config/doze/settings.toml`
  - Controls theme (Dark / Midnight / Light), editor font size, network defaults (timeout, redirects, TLS verification, proxy, User-Agent), global user variables, recent projects, and local request history.

- **Layered Variable Interpolation (`{{var}}`)**
  - Precedence: Built-ins (`{{$timestamp}}`, `{{$isoTimestamp}}`, `{{$uuid}}`, `{{$randomInt}}`) -> Active Env Local Secrets -> Active Env Shared Variables -> Global User Variables.
  - Live masked URL preview with automatic warnings for unresolved `{{vars}}`.

## Installing & Updating

Both Windows and macOS installers automatically **detect existing installations and update them in place** while preserving all user settings and local secrets.

### Windows
- **GUI Installer (from GitHub Releases)**: Run `Doze-Setup-x86_64.exe` (Inno Setup). Running a newer version updates your existing installation in place.
- **PowerShell Installer / Updater (from source or GitHub Releases)**:
  ```powershell
  # Build from this repo and install/update to %LOCALAPPDATA%\Programs\Doze
  .\installer\install.ps1

  # Or install/update directly from GitHub Releases
  .\installer\install.ps1 -Repo "Holyphoenix/doze"
  ```

### macOS (Apple Silicon & Intel Universal)
- **DMG Installer (from GitHub Releases)**: Open `Doze-macos-universal.dmg` and drag `Doze.app` into `Applications` (choose **Replace** when updating).
- **Shell Installer / Updater (from source or GitHub Releases)**:
  ```bash
  # Build from this repo and install/update /Applications/Doze.app
  ./installer/install.sh

  # Or install/update directly from GitHub Releases
  ./installer/install.sh --repo Holyphoenix/doze
  ```

## Running & Testing Locally

```bash
# Run the desktop application
cargo run

# Run the unit & integration test suite
cargo test
```

## GitHub Actions (CI & Releases)

- **CI** (`.github/workflows/ci.yml`): Runs `cargo test` and builds Windows (`Doze-Setup-x86_64.exe`, `.zip`) and macOS Universal (`Doze-macos-universal.dmg`, `.tar.gz`) artifacts on pushes and pull requests.
- **Releases** (`.github/workflows/release.yml`): Push a version tag (e.g. `git tag v0.1.0 && git push origin v0.1.0`) or trigger manually via **Actions → Release** to publish a GitHub Release with signed-checksum installers for both Windows and macOS.

You can immediately test opening a source-controlled `.doze` project using the bundled sample at `examples/demo-service/`.
