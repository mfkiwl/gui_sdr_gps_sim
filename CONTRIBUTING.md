# Contributing to GUI SDR GPS Simulator

Thank you for your interest in contributing! This document explains how to get involved, what the project expects from contributors, and how to get a pull request merged.

---

## Table of contents

1. [Before you start](#before-you-start)
2. [Ways to contribute](#ways-to-contribute)
3. [Setting up the development environment](#setting-up-the-development-environment)
4. [Making changes](#making-changes)
5. [Code style and quality rules](#code-style-and-quality-rules)
6. [Submitting a pull request](#submitting-a-pull-request)
7. [Reporting bugs](#reporting-bugs)
8. [Legal note](#legal-note)

---

## Before you start

- Check the [issue tracker](https://github.com/okiedocus/gui_sdr_gps_sim/issues) to see if your idea or bug is already being discussed.
- For large changes (new features, architectural refactors), open an issue first to discuss the approach before writing code. This avoids wasted effort if the direction doesn't fit the project.
- Small fixes (typos, documentation, obvious bugs) can go straight to a pull request.

---

## Ways to contribute

- **Bug reports** — something doesn't work as described in the README.
- **Bug fixes** — a focused change that corrects a specific problem.
- **Documentation** — improving the README, inline doc comments, or adding examples.
- **New features** — additional SDR output types, route import formats, UI improvements, etc.
- **Testing** — trying the app with different hardware, GPS receivers, or operating systems and reporting what you find.
- **GNU Radio flow graphs** — improvements or additions to the `gnuradio/` folder.

---

## Setting up the development environment

### Requirements

| Tool | Version | Notes |
|---|---|---|
| Rust | **1.88** | Installed automatically via `rustup` from `rust-toolchain` |
| HackRF drivers | latest | Only needed to test RF transmission |
| OpenRouteService API key | — | Only needed to test the ORS route source |

### Clone and build

```bash
git clone https://github.com/okiedocus/gui_sdr_gps_sim
cd gui_sdr_gps_sim
cargo run
```

The Rust toolchain (1.88) is picked up automatically from the `rust-toolchain` file.

### Linux dependencies

```bash
sudo apt-get install -y \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libssl-dev libudev-dev libgtk-3-dev
```

---

## Making changes

1. **Fork** the repository and create a branch from `main`:
   ```bash
   git checkout -b fix/my-bug-fix
   ```

2. **Make your changes.** Keep each pull request focused on a single concern. Avoid bundling unrelated fixes together.

3. **Run the full CI check locally** before pushing:
   ```bash
   bash check.sh
   ```
   This runs `cargo check`, `cargo fmt --check`, `cargo clippy` (zero warnings), and `cargo test`. All four must pass.

4. **Format your code:**
   ```bash
   cargo fmt --all
   ```

---

## Code style and quality rules

The project enforces strict linting. A pull request that introduces any Clippy warning will fail CI.

| Rule | Detail |
|---|---|
| **No `unwrap()`** | Use `?`, `if let`, or `.unwrap_or_default()` |
| **No `println!` / `eprintln!`** | Use `log::info!`, `log::warn!`, `log::error!` |
| **No `todo!()`** | Finish the implementation before opening a PR |
| **No wildcard imports** | Write explicit `use` paths |
| **Use `#[expect]` not `#[allow]`** | `#[expect(lint, reason = "…")]` is required when suppressing a lint |
| **No `unsafe` code** | `unsafe_code` is denied at the workspace level |
| **Avoid over-engineering** | Don't add abstractions, fallbacks, or configuration that isn't needed yet |

### Commit messages

Write commit messages in the imperative mood and keep the subject line under 72 characters:

```
fix: correct route speed on high-density ORS segments
feat: add PlutoSDR output type
docs: expand simulator settings table in README
```

---

## Submitting a pull request

1. Push your branch and open a pull request against `main`.
2. Fill in the PR description: what the change does and why, and how you tested it.
3. CI will run automatically (format, lint, tests on Linux). All checks must be green.
4. A maintainer will review the PR. Please be responsive to feedback — PRs with no activity for 30 days may be closed.

---

## Reporting bugs

Open an issue and include:

- **Operating system and version**
- **Rust toolchain version** (`rustc --version`)
- **Hardware** (HackRF serial number or model if relevant)
- **Steps to reproduce** — be as specific as possible
- **Expected behaviour** vs **actual behaviour**
- **Logs or error output** (run with `RUST_LOG=debug cargo run` for verbose output)

---

## Legal note

By submitting a contribution you agree that your code will be distributed under the terms of the [GNU General Public License v3.0 or later](LICENSE) that covers this project.

Transmitting GPS signals without authorisation is regulated or prohibited in most jurisdictions. Contributions that make it easier to misuse this software (e.g. circumventing the legal warning, automating mass transmission) will not be accepted.
