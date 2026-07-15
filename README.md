# Blackjack

Multiplayer Blackjack — domain core, WebSocket server, TUI client.

![Demo](assets/demo.gif)

## Architecture

```
blackjack/
├── core/      # Pure domain logic (no I/O) — game engine, rules, events
├── server/    # Axum HTTP + WebSocket server — auth, wallet, session
└── cli/       # Ratatui TUI client
```

**Stack**: Rust · Axum · Tokio · Ratatui · WebSockets

## Quick Start

### Local

```bash
# Terminal 1 — server
cargo run -p server

# Terminal 2 — client
cargo run -p cli
```

### Against hosted server

```bash
SERVER_URL=https://blackjack.skh.rs cargo run -p cli
```

### Raspberry Pi (no compilation needed)

Every `cli-v*` release ships a prebuilt aarch64 binary (built by
[`pi-binary.yml`](.github/workflows/pi-binary.yml), requires a 64-bit Pi OS):

Grab the newest `cli-v*` tag from the [releases page](https://github.com/skharchikov/blackjack/releases) (releases for other packages don't carry the binary), then:

```bash
curl -sL https://github.com/skharchikov/blackjack/releases/download/cli-v0.1.1/blackjack-cli-aarch64-linux.tar.gz | tar xz
SERVER_URL=https://blackjack.skh.rs ./blackjack-cli
```

### Accounts

Enter any username + password on the login screen — the account is auto-created on first login. Same credentials on subsequent logins authenticate you.

Pre-seeded accounts: `admin`, `qa`, `dev` — all with password `famly1234`.

## Gameplay

| Key | Action |
|-----|--------|
| `↑ ↓` | Navigate lobby |
| `Enter` | Join table as observer |
| `t` | Take a seat (auto-assigned) |
| `l` | Leave seat / leave table |
| `← →` | Adjust bet |
| `Enter` | Confirm bet |
| `h` | Hit |
| `s` | Stand |
| `q` | Quit |

## Deployment

Server runs on Hetzner via Docker. Deploy triggers automatically on push to `master`.

```bash
# Manual redeploy
gh workflow run deploy.yml
```

## Release Management

Uses [release-plz](https://release-plz.dev/) for automated versioning on `master`:

- `feat:` → minor bump
- `fix:` → patch bump
- `feat!:` / `fix!:` → major bump
