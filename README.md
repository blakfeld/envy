# envy

A declarative developer environment manager. Define your project's dependencies, services, environment variables, and runnable commands in a single `envy.yml` file — then run `envy up` to get everything running.

## What it does

- **Installs dependencies** via the platform package manager (languages, databases, CLIs, etc.)
- **Starts services** like MySQL and Redis, and waits for them to be healthy
- **Sets environment variables** persistently in your shell session via [shadowenv](https://shopify.github.io/shadowenv/)
- **Decrypts secrets** from [ejson](https://github.com/Shopify/ejson) files and merges them into the environment
- **Locks versions** in `envy.lock` so teammates get the same setup
- **Runs project commands** defined in `envy.yml` (like `npm run dev`, `make test`, etc.)

## Platform support

| Platform | Package manager | Service management |
|---|---|---|
| macOS | [Homebrew](https://brew.sh) (auto-installed if missing) | `brew services` |
| Ubuntu / Debian | apt (`sudo apt-get`) | `systemctl` |
| Windows 10/11 | [WinGet](https://learn.microsoft.com/en-us/windows/package-manager/winget/) | `net start` / `sc` |

## Requirements

- A supported platform (see above)
- Rust toolchain to build from source

## Installation

```sh
cargo install --path .
```

Add the shell hook to your rc file. It activates the environment after `envy up` **and** enables tab-completion for all built-in subcommands and your project's custom commands:

```sh
# ~/.zshrc
eval "$(envy hook zsh)"

# ~/.bashrc
eval "$(envy hook bash)"

# ~/.config/fish/config.fish
envy hook fish | source
```

## Quick start

Create a starter config:

```sh
envy init
```

Then edit `envy.yml` to add your dependencies, and bring the environment up:

```sh
envy up
```

## envy.yml reference

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

  # Profile-specific — only installed in the staging profile
  - some-staging-dep:
      profiles: [staging]

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

# Path to an ejson file whose decrypted values are merged into the environment.
# Secret values override plain environment variables on conflict.
secrets: secrets.ejson

commands:
  # Simple form — runs via `sh -c`
  dev: "npm run dev"

  # Configured form — custom shell, working directory, and profile restriction
  migrate:
    cmd: "bundle exec rails db:migrate"
    cwd: ./api
    shell: bash
    profiles: [dev, staging]

hooks:
  before_up: "echo 'Starting up…'"
  after_up:
    cmd: "bundle install"
    shell: bash
  before_down: ~
  after_down: "echo 'All done'"
```

## Commands

### `envy up`

Installs dependencies, starts services, and configures the environment.

```sh
envy up                   # Use the dev profile (default)
envy up --profile staging # Use a named profile
envy up --update          # Re-resolve all versions and rewrite envy.lock
envy up --dry-run         # Check status without making any changes
```

### `envy down`

Stops all managed services.

```sh
envy down
envy down --profile staging
```

### `envy status`

Shows what is installed, what services are running, and what environment variables are set.

```sh
envy status
```

### `envy check`

Validates that everything matches `envy.yml` and exits non-zero if any issues are found. Suitable for CI.

```sh
envy check
```

### `envy init`

Creates an empty `envy.yml` in the current directory.

```sh
envy init          # Fails if envy.yml already exists
envy init --force  # Overwrite an existing envy.yml
```

### `envy <command>`

Runs a command defined under `commands:` in `envy.yml`.

```sh
envy dev      # Runs the "dev" command
envy migrate  # Runs the "migrate" command
```

The active profile is read from the `ENVY_PROFILE` environment variable (defaults to `dev`), so profile-restricted commands are correctly filtered.

### `envy hook <shell>`

Prints a shell integration snippet. Pipe it to `eval` in your rc file (see [Installation](#installation)).

```sh
envy hook zsh
envy hook bash
envy hook fish
```

The snippet does two things:

1. **Shadowenv activation** — wraps `envy up` so the new environment is activated in your current shell session immediately after installation.
2. **Tab completion** — registers completion for all built-in subcommands (`up`, `down`, `status`, `check`, `init`, `hook`) and flags. Commands you define under `commands:` in `envy.yml` are completed **dynamically** — the completion function calls `envy _commands` at tab-press time so new commands appear without reloading your shell.

## Profiles

Profiles let you vary which dependencies and commands are active per environment. The default profile is `dev`. Pass `--profile <name>` to any command, or set `ENVY_PROFILE` for commands run via `envy <command>`.

```yaml
dependencies:
  - node          # active in all profiles
  - mysql:
      profiles: [dev, test]   # only in dev and test
  - some-prod-tool:
      profiles: [production]  # only in production

commands:
  seed:
    cmd: "bundle exec rails db:seed"
    profiles: [dev]           # only available in dev
```

## Secrets with ejson

Secrets are stored encrypted in an ejson file and decrypted at `envy up` time. Values are merged into the environment after plain `environment:` variables, so secrets win on conflict. Secret values are never printed to the terminal.

```sh
# Generate a keypair
ejson keygen

# Add the public key to your secrets file
# Encrypt and commit secrets.ejson — the private key stays out of source control
```

```yaml
# envy.yml
secrets: secrets.ejson
```

```json
// secrets.ejson
{
  "_public_key": "<your-public-key>",
  "DATABASE_PASSWORD": "EJ[1:...]",
  "API_KEY": "EJ[1:...]"
}
```

## Lock file

`envy up` writes `envy.lock` recording the exact version of every dependency that was installed. On subsequent runs without `--update`, envy pins each versionless dependency to its locked version so the environment is reproducible across machines.

Commit `envy.lock` to version control. Run `envy up --update` when you want to upgrade.

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

Each module knows the correct package name for each platform — you always use the same name in `envy.yml` regardless of OS:

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

**Ubuntu/Debian:** Install operations use `sudo apt-get`. Version pinning with the `version:` field uses apt's exact-version syntax (`pkg=version`) — for most languages, omit the version field and rely on `envy.lock` to pin the installed version across machines.

**Windows:** Service management uses `net start`/`sc`. Custom MySQL/PostgreSQL config options (`port`, `cli_args`) are not applied on Windows.
