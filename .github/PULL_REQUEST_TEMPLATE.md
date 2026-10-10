<!--
The PR title becomes part of the release notes and decides the version bump.
Use Conventional Commits: `feat: ...`, `fix: ...`, `docs: ...`, `feat!: ...`
-->

## Summary

<!-- What does this change and why? Link related issues: "Closes #123". -->

## Type of change

- [ ] `feat` - new feature (minor release)
- [ ] `fix` - bug fix (patch release)
- [ ] Breaking change (`!` / `BREAKING CHANGE:`)
- [ ] `docs` / `refactor` / `perf` / `test` / `build` / `ci` / `chore`

## Components affected

- [ ] `server` (hub)
- [ ] `daemon` (wnetd)
- [ ] `desktop` (client UI / Tauri backend)
- [ ] Shared crates (`crates/`)
- [ ] CI / release pipeline

## How was this tested?

<!-- Commands run, manual steps, screenshots for UI changes. -->

## Checklist

- [ ] CI is green.
- [ ] Tests added or updated where it makes sense.
- [ ] Documentation and config comments updated.
- [ ] No secrets, build output, or local tooling files committed.
- [ ] Wire-protocol changes stay compatible with the previous release, or are marked as breaking.
