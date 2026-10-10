# Contributing to Warlay

Thanks for taking the time to contribute! This guide covers how to set up the
project, the workflow we follow, and what a pull request needs before it can
be merged.

By participating you agree to follow our [Code of Conduct](CODE_OF_CONDUCT.md).
Security issues must be reported privately, see [SECURITY.md](SECURITY.md).

## Development setup

| Tool                             | Version         | Needed for                |
| -------------------------------- | --------------- | ------------------------- |
| [Rust](https://rustup.rs)        | stable          | every component           |
| [Node.js](https://nodejs.org)    | 20 LTS or newer | `desktop/`                |
| Windows 10/11 + MSVC build tools | -               | `daemon/` and `desktop/`  |
| Docker                           | optional        | building the server image |

The server and the shared crates build on Linux, macOS and Windows. The
daemon and the desktop client are Windows-only (raw LAN capture relies on
Winsock `SIO_RCVALL`).

```sh
git clone https://github.com/jhonsferg/warlay.git
cd warlay

# server + shared crates
cargo test -p server -p protocol -p crypto -p matchmaking -p core -p storage -p config -p transport

# desktop client (Windows)
cd desktop
npm ci
npm run tauri dev
```

See the [README](README.md#building-from-source) for full build instructions.

## Workflow

`main` is protected: every change lands through a pull request that passes CI
and gets a review. Merging a pull request into `main` **publishes a new
release automatically**, so keep `main` releasable at all times.

1. Create a branch from `main` named `type/short-description`, for example
   `feat/room-filters` or `fix/tunnel-reconnect`.
2. Make small, focused commits (one logical change per commit, ideally one
   file per commit).
3. Open a pull request using the template and make sure CI is green.
4. Address review feedback with new commits; do not force-push a branch
   someone else is reviewing.

## Commit messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org):

```
type(optional-scope): short imperative summary

Optional body explaining what and why, not how.
```

| Type                                                                | Use for                   | Release bump              |
| ------------------------------------------------------------------- | ------------------------- | ------------------------- |
| `feat`                                                              | a new user-facing feature | minor                     |
| `fix`                                                               | a bug fix                 | patch                     |
| `perf`, `refactor`, `docs`, `style`, `test`, `build`, `ci`, `chore` | everything else           | patch                     |
| `type!:` or a `BREAKING CHANGE:` footer                             | incompatible changes      | major (minor while < 1.0) |

The release pipeline reads these types to compute the next version and to
group the release notes, so the type matters. Keep the subject under 72
characters, and never commit secrets, build output, or local tooling and
editor files (see `.gitignore`).

## Formatting

Code and documentation are formatted automatically, and CI rejects
unformatted changes:

```sh
cargo fmt --all                    # Rust (rustfmt.toml)
npx prettier@3.3.3 --write .       # TypeScript, Vue, SCSS, JSON, YAML, Markdown (.prettierrc.json)
```

## Pull request checklist

- [ ] The branch is up to date with `main`.
- [ ] Code and docs are formatted (see [Formatting](#formatting)).
- [ ] `cargo test` passes for the crates you touched.
- [ ] For `desktop/` changes: `npm run build` (type-check + bundle) passes.
- [ ] New behavior is covered by tests where practical.
- [ ] Documentation (README files, config comments) is updated.
- [ ] No unrelated changes are mixed in.

## Project layout

See [Repository layout](README.md#repository-layout) in the README. In short:
shared logic lives in `crates/`, and `server/`, `daemon/` and
`desktop/src-tauri/` are thin binaries on top of it, consumed through the
`warlay` facade crate.

## Repository settings (maintainers)

To enforce this workflow, configure branch protection for `main`:

- Require a pull request before merging, with at least one approval.
- Require the status checks from the **CI** workflow to pass.
- Do not allow bypassing the rules above.
- Allow GitHub Actions to create releases:
  `Settings > Actions > General > Workflow permissions > Read and write`.
