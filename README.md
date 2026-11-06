# Space Scuffle

Space Scuffle is a real-time strategy game in the spirit of *StarCraft II*: deterministic simulation, crisp unit
control, high-ground vision, turn rates and group movement. Every mobile unit is a glass bubble and every
structure a dome, each playing a video clip of its creature or building ([`DESIGN.md`](DESIGN.md) §1). It is
written in Rust with Bevy, and is meant to be a reference for how to build a large, deterministic, real-time
project ([`AGENTS.md`](AGENTS.md) §1).

<a href="https://close2.github.io/space-scuffle/"><img src="https://close2.github.io/space-scuffle/showcase.webp" width="800" alt="Space Scuffle showcase: a real battle rendered by CI"></a>

The teaser above links to the [full video](https://close2.github.io/space-scuffle/). It is rendered by CI from the
latest green `main` with the real engine on software Vulkan ([workflow](.github/workflows/showcase.yml)); its
closing card is generated from [`DESIGN.md`](DESIGN.md) and the [plan index](docs/plans/README.md).

## Where it is

Every subsystem's status, the test that shows it working and its known gaps are in the table of
[`DESIGN.md` §4](DESIGN.md#4-subsystem-status); the video's closing card counts that table.

## Where it is heading

Active and proposed work is in the [plan index](docs/plans/README.md); decisions waiting for the owner are in
[`docs/questions/`](docs/questions/). Multiplayer over deterministic lockstep, replays of real matches and a bot
API are intent, not built.

## Build and run

On Linux with Rust (the toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml)) and the system
packages CI installs ([`ci.yml`](.github/workflows/ci.yml)):

```bash
cargo run --release -p client -- --skirmish
```

Contributors start with the engineering charter, [`AGENTS.md`](AGENTS.md).

## Licence

MIT OR Apache-2.0 (workspace `Cargo.toml`).
