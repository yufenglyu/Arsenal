<p align="center"><img src="public/arsenal.svg" width="80" alt="Arsenal"></p>

# Arsenal

**Keep your AI skills in one library, ready to use across tools and projects.**

Arsenal is a desktop application for collecting, organizing, installing and maintaining AI skills. Browse your library, see where skills are installed, review upstream changes and keep notes about what works for you—all in one workspace.

[简体中文](README_CN.md) · [Download](https://github.com/yufenglyu/Arsenal/releases) · [Changelog](CHANGELOG.md) · [Report an issue](https://github.com/yufenglyu/Arsenal/issues)

![Skill library with nested groups, tags and installation targets](docs/images/product/library-en.png)

*Screenshots use actual application components with fictional demonstration data. Example skills are not bundled.*

## Why Arsenal?

Collecting skills is easy. Finding the right one later, understanding when to use it and keeping it available across AI tools takes more work.

| The problem | How Arsenal helps |
| --- | --- |
| Skills are scattered across GitHub, downloads and projects | Import from GitHub or local folders and browse repositories and skills together |
| Your collection becomes difficult to navigate | Organize it with nested groups, tags and search |
| Several AI tools need the same skills | Manage shared, platform-specific and project-specific installations in one place |
| Upstream changes are hard to track | Check source changes and review results before synchronization |
| Practical knowledge gets forgotten | Read documentation, write remarks and link Obsidian notes |

## One library, multiple destinations

The skill library stores the original files. Choose where to install each skill: share frequently used capabilities, or build a focused set for a particular tool or project.

```text
GitHub / Local folders
        ↓
     Skill library
        ├── Groups and tags: organize and find
        └── Skill installation
              ├── Shared Hub
              ├── Software platforms
              └── Project directories
```

- **Collect in one place.** Import GitHub repositories or local skills, review the skill list and resolve conflicts during import.
- **Install by destination.** Choose platforms or projects and inspect installation relationships and counts.
- **Reduce duplicate copies.** Shared Hub links back to library skills. Platform installation prefers links and falls back to copying when necessary. Whether a tool reads a shared directory depends on that tool's support and configuration.
- **Bring your own tools.** Configure platform names and skill paths, enable the platforms you use and manage project directories.

## Organize around your work

Use groups for development, research, writing or other activities, and tags for purpose and characteristics. Drag skills or repositories into nested groups. Reordering, multiple selection and keyboard shortcuts help manage a growing library.

Switch between repository and flat skill views, and search names, descriptions and remarks. Adjustable columns and collapsible sidebars let you arrange the workspace while keeping documents and skill lists close at hand.

## Review changes and manage background work

Repository refresh and skill update checks serve different purposes. Refresh retrieves metadata, stars and documentation; update checks inspect upstream skill changes for review before synchronization.

Imports, checks and updates share a task center with progress and results. Filter failures or items needing attention, then retry individually or in batches without searching through completed work.

![Task center with failure filtering and retry controls](docs/images/product/tasks-en.png)

Update checks save progress per repository and support pause and continuation. After restarting the application, continue unfinished repositories without rechecking completed ones. Closing the task window leaves background work running.

## Keep what you learn

- **Read documentation** alongside skill details and repository information.
- **Write remarks and add tags** to record useful scenarios, limitations and your own assessment.
- **Use AI-assisted suggestions** for remarks and tags, then edit and save them. An AI service must be configured.
- **Connect Obsidian notes** for longer practical records, using stable identifiers to associate notes with skills or repositories.

## Keep control of your data

Skills and configuration live locally. Local backups, WebDAV backups and repository synchronization help preserve your library. Windows portable mode keeps `arsenal.exe` beside its `.arsenal` data directory.

Close the old application and back up your data before upgrading. Preserve your existing `.arsenal` directory rather than replacing it with packaged defaults. Remote imports, synchronization and AI features connect to their respective services.

## Get started

1. Download an installer or portable archive from [Releases](https://github.com/yufenglyu/Arsenal/releases).
2. Choose **Add skills** in the library and import a GitHub repository or local folder.
3. Create groups, add tags, read documentation and record useful remarks.
4. Configure destinations under **Skill installation**, then install skills into platforms or projects.
5. Check for upstream changes and open **Tasks** from the status bar to review progress and results.

Available platforms and package formats are listed in the release attachments.

## Development and contributions

The frontend uses React and TypeScript; the desktop application uses Tauri and Rust.

```bash
pnpm install
pnpm tauri dev
```

Use `pnpm dev` for frontend development. Validate with `pnpm test`, `pnpm typecheck` and `cargo test --manifest-path src-tauri/Cargo.toml`. See [build and packaging scripts](scripts/README.md) for distribution instructions.

Issues and focused improvements are welcome. Include your operating system, application version and reproduction steps, and keep sensitive information out of screenshots and logs.

Licensed under [Apache-2.0](LICENSE).
