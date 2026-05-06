# devy

[![crates.io](https://img.shields.io/crates/v/devy.svg)](https://crates.io/crates/devy)
[![CI](https://github.com/blakfeld/envy/actions/workflows/ci.yml/badge.svg)](https://github.com/blakfeld/envy/actions/workflows/ci.yml)

A declarative developer environment manager. Define your project's dependencies, services, environment variables, and runnable commands in a single `devy.yml` file — then run `devy up` to get everything running.

## What it does

- **Installs dependencies** via the platform package manager (languages, databases, CLIs, etc.)
- **Starts services** like MySQL and Redis, and waits for them to be healthy
- **Sets environment variables** persistently in your shell session via [shadowenv](https://shopify.github.io/shadowenv/)
- **Decrypts secrets** from [ejson](https://github.com/Shopify/ejson) files and merges them into the environment
- **Locks versions** in `devy.lock` so teammates get the same setup
- **Runs project commands** defined in `devy.yml` (like `npm run dev`, `make test`, etc.)

## Platform support

| Platform | Package manager | Service management |
|---|---|---|
| macOS | [Homebrew](https://brew.sh) (auto-installed if missing) | `brew services` |
| Ubuntu / Debian | apt (`sudo apt-get`) | `systemctl` |
| Windows 10/11 | [WinGet](https://learn.microsoft.com/en-us/windows/package-manager/winget/) | `net start` / `sc` |

## Requirements

- A supported platform (see above)

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
devy up
```

## devy.yml reference

```yaml
name: my-project

dependencies:
  # Simple form — installs the latest version
  - redis
  - jq

  # Pinned version
  - node:
      version: "20"

  # Custom Homebrew tap
  - my-tool:
      tap: myorg/homebrew-tools

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

## Commands

### `devy up`

Installs dependencies, starts services, and configures the environment.

```sh
devy up            # Set up the environment
devy up --update   # Re-resolve all versions and rewrite devy.lock
devy up --dry-run  # Check status without making any changes
```

### `devy down`

Stops all managed services.

```sh
devy down
```

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
| `mysql` | 3306 | Managed service; supports `port`, `cli_args`; writes `my.cnf` where supported |
| `postgresql`, `postgres` | 5432 | Managed service; supports `port` |
| `redis` | 6379 | Managed service; health-checks via PING |
| `mongodb`, `mongo` | 27017 | Managed service; on Homebrew requires `tap: mongodb/brew` |
| `nginx` | 80 | Managed service; supports `port` |

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
| `scala` | |
| `php` | |
| `elixir` | |
| `swift` | |
| anything else | Falls back to a generic package manager install |

### Platform package name mapping

Each module knows the correct package name for each platform — you always use the same name in `devy.yml` regardless of OS:

| Module | Homebrew | apt | WinGet |
|---|---|---|---|
| `mysql` | `mysql` | `mysql-server` | `Oracle.MySQL` |
| `postgresql` | `postgresql` | `postgresql` | `PostgreSQL.PostgreSQL` |
| `redis` | `redis` | `redis-server` | `Redis.Redis` |
| `mongodb` | `mongodb-community` | `mongodb-org` | `MongoDB.Server` |
| `nginx` | `nginx` | `nginx` | `Nginx.Nginx` |
| `node` | `node` | `nodejs` | `OpenJS.NodeJS` |
| `python` | `python` | `python3` | `Python.Python.3` |
| `go` | `go` | `golang-go` | `GoLang.Go` |
| `java` | `openjdk` | `default-jdk` | `Microsoft.OpenJDK.21` |
| `kotlin` | `kotlin` | `kotlin` | `JetBrains.Kotlin` |
| `ruby` | `ruby` | `ruby` | `RubyInstallerTeam.Ruby.3` |

### Platform notes

**Ubuntu/Debian:** Install operations use `sudo apt-get`. Version pinning with the `version:` field uses apt's exact-version syntax (`pkg=version`) — for most languages, omit the version field and rely on `devy.lock` to pin the installed version across machines.

**Windows:** Service management uses `net start`/`sc`. Custom MySQL/PostgreSQL config options (`port`, `cli_args`) are not applied on Windows.
