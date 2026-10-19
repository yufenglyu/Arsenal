# Arsenal

AI tools and capabilities management. Skills are supported today; templates, workflows and knowledge are future directions.

[简体中文](README_CN.md) · [Download](https://github.com/yufenglyu/Arsenal/releases) · [Changelog](CHANGELOG.md)

![Library and nested groups](docs/images/v1.3/library.png)

Screenshots show actual v1.3.0 frontend components in Chinese with fictional demonstration data, not bundled content.

## Organize and install

Import from GitHub or local folders. The library remains the primary storage. Shared skills, software platforms and project directories are grouped under Skill installation. Installation prefers links, with copying as a fallback.

Manage nested groups directly beneath the library. Drag skills or repositories into groups, reorder groups, use Shift to move membership, and use selection and expansion shortcuts. Groups support both skill and repository views. Removing membership does not delete the source skill. Create groups through the context menu.

Click section titles to collapse or expand. Resize the library section vertically. Clicking navigation icons keeps collapsed sidebars collapsed. Tables support configurable columns, widths and sorting; the skill-count column is not sortable.

## Updates and tasks

![Update dialog](docs/images/v1.3/update.png)

Refresh repository information updates metadata, stars and documentation without updating skill files. Check skill updates inspects upstream changes for review before synchronization. Open Tasks from the status bar to view previous results, filter failures and retry selected work.

Checks save progress per repository. Pause finishes the current repository before stopping. After restarting, choose Continue check in Tasks to skip completed repositories. A repository interrupted before completion is checked again. Keep the task record if you intend to resume it.

## Notes and portable data

Use tags, remarks and AI-assisted suggestions to organize skills. Obsidian notes retain stable identifiers. The new-note subdirectory is relative to the vault, for example `附件/SkillsHub`.

Windows portable installations need `arsenal.exe` and `.arsenal`. Close the old application before upgrading and preserve your existing data directory. Never overwrite it with packaged defaults.

Migration rewrites legacy `.skillshub` paths and installed links, validates them, then removes the compatibility directory. Two independent data directories block migration. Backups remain under `.arsenal/migrations`. Existing protocols, note properties and upgrade identifiers remain compatible.

## Development

Run `pnpm install`, then `pnpm dev` for the frontend or `pnpm tauri dev` for the desktop application.

Validate with `pnpm test`, `pnpm typecheck` and `cargo test --manifest-path src-tauri/Cargo.toml`.

Package Windows with `pnpm package:release:windows`; see [packaging instructions](scripts/README.md) for other platforms. Available release attachments are authoritative; local Windows builds do not verify macOS or Linux.

Licensed under [Apache-2.0](LICENSE).
