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
- [Architecture](ARCHITECTURE.md): the layers, the module order, and the file where each type of change starts, checked by `tests/architecture_map.rs`.
- [Browser development](../web/README.md): browser preparation, locked builds, and Playwright checks.
- [Engine benchmarks](BENCHMARKS.md): repeatable measurements and interpretation.
- [Project-owned proofs](PROJECT-PROOFS.md) explains how to check a compiled word and review changes to its shape and positioning in CI.
- [Review a spacing proposal in Agent Proof](AGENT-SPACING.md#review-a-proposed-change-in-the-editor) walks through connecting an agent, comparing real words, and accepting or rejecting a change.
  The [spacing-pass agent instructions](../skills/runebender-spacing-pass/SKILL.md) give the tool sequence and revision checks.
- [Audit of 2026-11-09](AUDIT-2026-11-09.md): historical findings, status corrections, and the previously reported validation baseline.
- [Cleanup plan](CLEANUP-PLAN.md): a checklist from the code review of 2026-11-17, with files, changes and verification for each item.
- [Quality review of 2026-11-23](QUALITY-REVIEW-2026-11-23.md): a temporary checklist from a full review of the repository, one item at a time.
- [Engineering review of 2026-11-25](QUALITY-PASS-2026-11-25.md) records the fixes, verification and remaining risks from the repository-wide pass.
- [Upstream work](upstream/README.md): changes and workarounds to propose to Xilem, Vello and `spline`, with the Linebender LLM policy rules.

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

- [Arabic text layout: research and plan](ARABIC-TEXT-LAYOUT-PLAN.md): the comparison with Fontra, Counterpunch and Glyphs, measurements, the design that computes layout from live values, and the work checklist.
- [Higher-order interpolation research and plan](HOI-RESEARCH-PLAN.md): research, accepted design decisions, the synthetic test font, and the phased implementation plan.
- [Virtua Regular AI checklist](VIRTUA-REGULAR-AI-CHECKLIST.md)
- [AI architecture checklist](AI-ARCHITECTURE-CHECKLIST.md)

Keep repository-wide guides and checklists here, and add them to this index.
Component-specific README files stay beside the code or assets they describe.
