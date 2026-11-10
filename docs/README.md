# Runebender documentation

The [main README](../README.md) introduces the project and its local build commands.
The [contributor guide](CONTRIBUTING.md) covers setup, tests, source ownership, and review policy.
[AGENTS.md](../AGENTS.md) records detailed engineering instructions and links to the canonical architecture and design documentation on [runebender.org](https://runebender.org).
Commands in these guides run from the repository root unless stated otherwise.

## Releases and installation testing

- [Release plan and runbook](RELEASING.md): choosing a commit, packaging, signing, notarization, publishing, website downloads, and future distribution channels.
- [Testing a macOS DMG](TESTING-MACOS.md): the everyday build, install, upgrade, and validation workflow.
- [Changelog](CHANGELOG.md): changes being prepared and notes for published versions.
- [0.1.0-alpha.1 release notes](releases/0.1.0-alpha.1.md): the curated notes for the GitHub release and the update feed.

## Development and validation

- [Contributing](CONTRIBUTING.md): native prerequisites, focused checks, architecture boundaries, and narrow API review.
- [Browser development](../web/README.md): browser preparation, locked builds, and Playwright checks.
- [Engine benchmarks](BENCHMARKS.md): repeatable measurements and interpretation.
- [Audit of 2026-11-09](AUDIT-2026-11-09.md): historical findings, status corrections, and the previously reported validation baseline.

## Interface and local models

- [Interface design and headless screenshots](UI-DESIGN.md): shared panel conventions and visual validation.
- [Local AI reference](LOCAL-AI.md): every local AI feature, agent tool, runtime and model location, checked by `tests/local_ai_reference.rs`.
- [Proof reference fonts](PROOF-REFERENCES.md): the project-local folder shared with Virtua's live proof in Codex.
- [Local model packages](LOCAL-MODELS.md): package layout, runtime requirements, and installation.
- [External workflow worker contract](WORKER-CONTRACT.md): discovery, Rows transport, cache and trust policy, and candidate review.
- [Virtua Regular testing](VIRTUA-REGULAR-TESTING.md): scratch-font setup and manual review.
- [Steam Deck plan](STEAM-DECK.md): feasibility, SSH setup, local models, and controller mapping for a handheld.

## Planning and review records

These checklists retain implementation decisions, unfinished work, and earlier validation limits.
Check their dates and the current implementation before treating an item as completed or as the next task.

- [Higher-order interpolation research and plan](HOI-RESEARCH-PLAN.md): research, accepted design decisions, the synthetic test font, and the phased implementation plan.
- [Virtua Regular AI checklist](VIRTUA-REGULAR-AI-CHECKLIST.md)
- [AI architecture checklist](AI-ARCHITECTURE-CHECKLIST.md)

Keep repository-wide guides and checklists here, and add them to this index.
Component-specific README files stay beside the code or assets they describe.
