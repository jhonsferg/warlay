# Changelog

Release notes are generated automatically from
[Conventional Commits](https://www.conventionalcommits.org) every time a pull
request is merged into `main`, and published with each version on the
[Releases page](https://github.com/jhonsferg/warlay/releases).

Notes are grouped by commit type (features, bug fixes, performance,
refactoring, documentation, build and CI, maintenance) using the template in
[`.github/cliff.toml`](.github/cliff.toml). To preview the notes for the
unreleased changes locally:

```sh
git cliff --config .github/cliff.toml --unreleased
```

See [CONTRIBUTING.md](CONTRIBUTING.md#commit-messages) for how commit types
map to version bumps.
