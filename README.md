# devy

[![crates.io](https://img.shields.io/crates/v/devy.svg)](https://crates.io/crates/devy)
[![CI](https://github.com/blakfeld/envy/actions/workflows/ci.yml/badge.svg)](https://github.com/blakfeld/envy/actions/workflows/ci.yml)

A declarative developer environment manager. Define your project's dependencies, services, environment variables, and runnable commands in a single `devy.yml` file — then run `devy up` to get everything running.

## What it does

- **Installs dependencies** via Nix by default — packages land in `.devy/nix-profile` inside your project, not your global environment
- **Starts services** like MySQL and Redis, and waits for them to be healthy
- **Sets environment variables** persistently in your shell session via [shadowenv](https://shopify.github.io/shadowenv/)
- **Locks versions** in `devy.lock` so teammates get the same setup
- **Runs project commands** defined in `devy.yml` (like `npm run dev`, `make test`, etc.)

## Platform support

| Platform | Default package manager | Service management |
|---|---|---|
| macOS | [Nix](https://nixos.org) (project-local profile) | launchd (`launchctl`) |
| Ubuntu / Debian | [Nix](https://nixos.org) (project-local profile) | systemd user units (`systemctl --user`) |
| Windows 10/11 | [WinGet](https://learn.microsoft.com/en-us/windows/package-manager/winget/) | `net start` / `sc` |

On macOS and Linux you can opt into your system package manager instead by setting `package_manager: brew` or `package_manager: apt` in `devy.yml`. See [Choosing a package manager](#choosing-a-package-manager).

## Requirements

- A supported platform (see above)
- **macOS / Linux:** Nix is required. Run `devy up --bootstrap` to install it automatically via the [Determinate Installer](https://install.determinate.systems), or install Nix manually first.

## Installation

`devy` is published on [crates.io](https://crates.io/crates/devy). Install with:

```sh
cargo install devy
```

Add the shell hook to your rc file. It activates the environment after `devy up` **and** enables tab-completion for all built-in subcommands and your project's custom commands:

```sh
# ~/.zshrc
eval "$(devy hook zsh)"

# ~/.bashrc
eval "$(devy hook bash)"

# ~/.config/fish/config.fish
devy hook fish | source
```

## Quick start

Create a starter config:

```sh
devy init
```

Then edit `devy.yml` to add your dependencies, and bring the environment up:

```sh
devy up --bootstrap   # installs Nix automatically if not already installed
devy up               # if Nix is already installed
```

## devy.yml reference

```yaml
name: my-project

# Package manager to use. Defaults to "nix" on macOS and Linux, "winget" on Windows.
# Options: nix, brew (macOS only), apt (Linux only)
package_manager: nix

dependencies:
  # Simple form — installs the latest version
  - redis
  - jq

  # Pinned version
  - node:
      version: "20"

  # MySQL with a custom port and extra server flags
  - mysql:
      port: 3307
      cli_args: "--innodb-buffer-pool-size=256M"

  # Node with global npm packages
  - node:
      version: "20"
      global_packages:
        - typescript
        - eslint

  # Ruby with gems
  - ruby:
      gems:
        - rails
        - bundler

  # Rust with a specific toolchain, targets, and components
  - rust:
      toolchain: stable
      targets:
        - wasm32-unknown-unknown
      components:
        - rust-analyzer

environment:
  DATABASE_URL: "mysql://root@127.0.0.1:3306/myapp_dev"
  REDIS_URL: "redis://127.0.0.1:6379"
  LOG_LEVEL: "debug"

commands:
  # Simple form — runs via `sh -c`
  dev: "npm run dev"

  # Configured form — custom shell and working directory
  migrate:
    cmd: "bundle exec rails db:migrate"
    cwd: ./api
    shell: bash

hooks:
  # Single command
  before_up: "echo 'Starting up…'"

  # Configured command
  after_up:
    cmd: "bundle install"
    shell: bash

  # List of commands — run in order, stops on first failure
  before_down:
    - "echo 'Stopping…'"
    - cmd: "make teardown"
      shell: bash

  after_down: ~
```

## Choosing a package manager

devy defaults to Nix on macOS and Linux. Nix installs packages into a project-local profile at `.devy/nix-profile`, so nothing leaks into your global environment and each project is fully isolated.

To use your system package manager instead, set `package_manager:` in `devy.yml`:

```yaml
package_manager: brew   # macOS only — uses Homebrew
package_manager: apt    # Linux only — uses apt-get (requires sudo)
```

When `package_manager` is omitted or set to `auto`, devy prints a warning and falls back to Nix.

## Commands

### `devy up`

Installs dependencies, starts services, and configures the environment.

```sh
devy up               # Set up the environment
devy up --bootstrap   # Auto-install Nix if it is not already installed
devy up --update      # Re-resolve all versions and rewrite devy.lock
devy up --dry-run     # Check status without making any changes
```

### `devy down`

Stops all managed services.

```sh
devy down
```

### `devy services`, `devy start`, `devy stop`, `devy restart`

Manage individual services without touching the rest of the environment. Use these when you want to control a single service — restart a database after a config change, stop something you don't need right now, or bring a service back up without re-running `devy up` for everything.

```sh
devy services        # List all services and their current running status
devy start redis     # Start a service (skips if already running)
devy stop redis      # Stop a service (skips if already stopped)
devy restart mysql   # Stop then start a service, waiting for it to be healthy
```

Service names match what's defined under `dependencies:` in `devy.yml`.

### `devy status`

Shows what is installed, what services are running, and what environment variables are set.

```sh
devy status
```

### `devy check`

Validates that everything matches `devy.yml` and exits non-zero if any issues are found. Suitable for CI.

```sh
devy check
```

### `devy init`

Creates an empty `devy.yml` in the current directory.

```sh
devy init          # Fails if devy.yml already exists
devy init --force  # Overwrite an existing devy.yml
```

### `devy export`

Exports the environment as a Nix file so you can use it with `nix-shell` or Nix flakes outside of devy.

```sh
devy export                    # Writes flake.nix (default)
devy export --format=shell     # Writes shell.nix
devy export --format=flake     # Writes flake.nix
```

### `devy pr`

Opens a GitHub pull request for the current branch in your browser.

```sh
devy pr
```

### `devy <command>`

Runs a command defined under `commands:` in `devy.yml`.

```sh
devy dev      # Runs the "dev" command
devy migrate  # Runs the "migrate" command
```

### `devy hook <shell>`

Prints a shell integration snippet. Pipe it to `eval` in your rc file (see [Installation](#installation)).

```sh
devy hook zsh
devy hook bash
devy hook fish
```

The snippet does two things:

1. **Shadowenv activation** — wraps `devy up` so the new environment is activated in your current shell session immediately after installation.
2. **Tab completion** — registers completion for all built-in subcommands (`up`, `down`, `status`, `check`, `init`, `hook`) and flags. Commands you define under `commands:` in `devy.yml` are completed **dynamically** — the completion function calls `devy _commands` at tab-press time so new commands appear without reloading your shell.

## Lock file

`devy up` writes `devy.lock` recording the exact version of every dependency that was installed. On subsequent runs without `--update`, devy pins each versionless dependency to its locked version so the environment is reproducible across machines.

Commit `devy.lock` to version control. Run `devy up --update` when you want to upgrade.

## Supported dependency modules

### Services

| Name(s) | Default port | Notes |
|---|---|---|
| `mysql` | 3306 | Managed service; supports `port`, `cli_args` |
| `postgresql`, `postgres` | 5432 | Managed service; supports `port` |
| `redis` | 6379 | Managed service; health-checks via PING |
| `mongodb`, `mongo` | 27017 | Managed service |
| `nginx` | 80 | Managed service; supports `port` |
| `mariadb` | 3306 | Managed service |
| `rabbitmq` | — | Managed service |
| `memcached` | — | Managed service |
| `minio` | — | Managed service |
| `vault` | — | Managed service |
| `elasticsearch` | — | Managed service |
| `kafka` | — | Managed service |
| `meilisearch` | — | Managed service |
| `mailhog` | — | Managed service |
| `opensearch` | — | Managed service |

### Languages and runtimes

| Name(s) | Notes |
|---|---|
| `node`, `nodejs`, `javascript`, `js` | Supports `global_packages` |
| `typescript`, `ts` | Installs Node + TypeScript globally; supports `global_packages` |
| `ruby` | Supports `gems` |
| `rust`, `rustup` | Installs via rustup (all platforms); supports `toolchain`, `targets`, `components` |
| `python`, `python3` | |
| `go`, `golang` | |
| `java`, `openjdk` | |
| `kotlin` | |
| `elixir` | |
| `erlang` | |
| `dart` | |
| `crystal` | |
| `zig` | |
| `bun` | |
| `deno` | |
| `dotnet` | |
| `swift` | |
| anything else | Falls back to a generic package manager install |

### Package name mapping

Each module knows the correct package name for each package manager — you always use the same name in `devy.yml` regardless of which backend is active:

| Module | Nix (`nixpkgs`) | Homebrew | apt | WinGet |
|---|---|---|---|---|
| `mysql` | `mysql80` | `mysql` | `mysql-server` | `Oracle.MySQL` |
| `postgresql` | `postgresql` | `postgresql` | `postgresql` | `PostgreSQL.PostgreSQL` |
| `redis` | `redis` | `redis` | `redis-server` | `Redis.Redis` |
| `mongodb` | `mongodb` | `mongodb-community` | `mongodb-org` | `MongoDB.Server` |
| `nginx` | `nginx` | `nginx` | `nginx` | `Nginx.Nginx` |
| `node` | `nodejs` | `node` | `nodejs` | `OpenJS.NodeJS` |
| `python` | `python3` | `python` | `python3` | `Python.Python.3` |
| `go` | `go` | `go` | `golang-go` | `GoLang.Go` |
| `java` | `jdk` (version-matched) | `openjdk` | `default-jdk` | `Microsoft.OpenJDK.21` |
| `kotlin` | `kotlin` | `kotlin` | `kotlin` | `JetBrains.Kotlin` |
| `ruby` | `ruby` | `ruby` | `ruby` | `RubyInstallerTeam.Ruby.3` |

### Platform notes

**macOS (Nix default):** Services are managed via launchd. devy writes a `LaunchAgent` plist to `~/Library/LaunchAgents/sh.devy.<name>.plist` and uses `launchctl` to start and stop them.

**Linux (Nix default):** Services are managed via systemd user units. devy writes a unit file to `~/.config/systemd/user/devy-<name>.service` and uses `systemctl --user` to start and stop them — no `sudo` required.

**macOS (Homebrew):** Set `package_manager: brew` in `devy.yml`. Service management uses `brew services`.

**Ubuntu/Debian (apt):** Set `package_manager: apt` in `devy.yml`. Install operations use `sudo apt-get`. Version pinning with the `version:` field uses apt's exact-version syntax (`pkg=version`) — for most languages, omit the version field and rely on `devy.lock` to pin the installed version across machines.

**Windows:** Service management uses `net start`/`sc`. Custom MySQL/PostgreSQL config options (`port`, `cli_args`) are not applied on Windows. Nix is not supported on Windows.

## Security

### `after_install`

`devy.yml` supports an `after_install` field that runs an arbitrary shell command immediately after a dependency is freshly installed:

```yaml
dependencies:
  - mysql:
      after_install: "mysql_secure_installation"
```

**This executes arbitrary code on your machine.** devy prints a warning to the terminal before executing each `after_install` command so you can see what is about to run. Before running `devy up` on a project you did not author — especially one shared via a template or onboarding flow — review all `after_install` values in `devy.yml`.

This is the same attack surface as `npm install` lifecycle scripts or `pip install` running `setup.py`.

### Nix auto-install

If Nix is not installed on macOS or Linux, `devy up --bootstrap` will install it by fetching and executing the [Determinate Installer](https://install.determinate.systems) over HTTPS. Without `--bootstrap`, devy exits with an error and instructions to install Nix manually:

```sh
devy up --bootstrap   # allows automatic Nix installation
devy up               # exits with an error if Nix is not installed
```

This is recommended in CI environments where unexpected system-level changes should be blocked — omit `--bootstrap` and pre-install Nix in your CI image instead.

### Homebrew `tap:` field

When using `package_manager: brew`, `devy.yml` accepts a `tap:` field to pull packages from a custom Homebrew tap:

```yaml
package_manager: brew
dependencies:
  - my-tool:
      tap: myorg/homebrew-tools
```

Only allow taps from sources you trust — tapping a malicious repository can execute code during the tap install. This field has no effect when using the Nix backend.

### `devy.yml` scope

devy walks up the directory tree to find `devy.yml` but stops at the nearest `.git` root (or `$HOME`). It will not read `devy.yml` files from parent directories outside the current git repository, preventing a malicious config in a parent directory from being executed.
