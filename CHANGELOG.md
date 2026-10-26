# Changelog

[English](CHANGELOG.md) | [简体中文](CHANGELOG.zh.md)

## 1.5.2 - 2026-10-26

- Restore WebDAV backups entirely in the backend and pass native local backup paths instead of archive bytes through the WebView. Report small progress messages rather than serializing large numeric arrays.
- Stage ZIP entries on disk with CRC, path and size checks before restoration; avoid whole-archive buffers and Base64 copies of ZIP payloads, clean temporary files on failures, and explain download, validation and disk-space failures.
- Limit archive inputs to 1 GiB, expanded ZIP data to 16 GiB and 100000 entries, and manifests or standalone JSON backups to 128 MiB. Synchronize the settings-page version and Linux package metadata with this release.

## 1.5.1 - 2026-10-26

- Stop deriving remote skill paths from local directory names. Flattened, renamed and hash-suffixed import folders now retain the actual source manifest recorded during import and subsequent scans and updates.
- Block checks and synchronization when the source path is missing, preserving installed content and explaining that the repository must be imported again to restore source metadata. Do not misreport missing metadata as a removed upstream source or propose unverified additions and deletions.
- Add offline regression coverage for same-name imports, hashed and renamed destinations, repeated scans, updates, and missing-source checks and apply attempts. Document the source identity contract.

- Unify note properties, navigation protocols, Obsidian plugin identity, browser storage, backup names and packaging paths under Arsenal. Remove old-brand aliases and startup seeding guards; existing old-brand data is neither migrated nor used as a fallback.
- Use a new `arsenal_property_template` setting with Arsenal defaults. Existing saved templates are not reused. The companion Obsidian plugin is now 0.3.0 and installs as `arsenal-link`.

## 1.5.0 - 2026-10-25

- Remember the main window's size, position and maximized state. Closing hides the app to the system tray; left-click restores it, and the right-click menu offers Show main window and Quit. Confirm quitting while tasks are active.
- Add a repository update workspace with a left filter sidebar, compact change summaries and consistent icon actions. Keep Current tasks, History and a shared operation Log in the task header; remove task search boxes and redundant footer hints.
- Put risk notices and approval on one title row. Approval changes the repository state to Confirmed, selects it for synchronization and collapses its details; rechecking a changed snapshot requires a fresh approval.
- Show recognized failure causes and recovery advice for imports, checks, metadata refreshes, synchronization and AI tasks. Preserve backend error codes, reject missing check results, and report view-refresh warnings separately from completed writes.
- Fix repository metadata refreshes for shared-directory links into the library. Retry counts now describe settled failures or expired results rather than queued checks.
- Retry transient Windows file-sharing failures during atomic metadata publication, retaining the committed file and reporting persistent failures after a bounded wait.
- Retain selected source variants during updates, deduplicate identical skill packages, support categorized directories and moved sources, and exclude script test fixtures and ordinary lowercase guides from skill discovery. Keep a previously installed root package scoped when its source moves.
- Add recoverable deletion and repository reimport through the recycle bin, including binary payload archives, skill-name filtering and safer conflict handling.
- Refine library toolbar geometry, group selection, sidebar counts and notebook controls. Use Arsenal branding in notebook controls.
- Update bilingual usage documentation and screenshots with fictional data, including update review, actionable failures and the unified log.

## 1.4.1 - 2026-10-19

- Remove automatic legacy configuration and installation-link migration, which could abort startup on macOS when an old link points to missing data.
- Use current `.arsenal` configuration directly and ignore obsolete configuration pointers; leave old directories and links untouched.
- Remove the legacy path migration utility and obsolete packaging guards. Add packaged macOS startup checks for dangling links and coexisting old data.

## 1.4.0 - 2026-10-19

- Redesign Tasks around batch summaries and expandable rows. Select imported skills, resolve conflicts and review repository updates inline, with current tasks and history in separate tabs.
- Show individual and batch repository refreshes, with progress, stop, resume and failed-item retries. Keep AI remarks and explanations out of the task window.
- Fix update checks failing when snapshot storage reaches capacity. Compress and deduplicate snapshots, avoid storing unchanged payloads, and release completed update payloads.
- Automatically collect unused snapshots at startup and after task state changes, while retaining snapshots needed by active tasks, pending reviews and recovery.
- Preserve error codes and show actionable failure details instead of a generic operation-failed message.
- Rewrite the Chinese and English READMEs with six illustrated workflows each, and fix English install-button plural forms.

## 1.3.0 - 2026-10-19

- Rebrand as Arsenal with a new icon and simplified brand header.
- Integrate nested groups, drag and drop, and multiple selection into the skill library.
- Group shared skills, platforms and projects under Skill installation.
- Refine sidebars, context menus, update confirmation and task dialogs.
- Pause update checks and continue unfinished repositories after restarting.
- Migrate configuration and installed links to `.arsenal`, preserving backups and upgrade compatibility.
- Refresh documentation and screenshots with fictional demonstration data.

## 1.2.0 - 2026-10-10

- Redesign Tasks with Skill imports / Skill updates tabs, a task list and a details pane. Keep the existing theme, add title/help/cleanup icons, and retain selections across tabs. Start checks from the main toolbar; closing the dialog keeps tasks running.
- Default update results to actionable changes. Show separate Failed, Updated, Unchanged and Ignored filters, with All last; remove the update-results search field. Confirm source deletions, support individual failed-check retries and dismissal, and preserve skill files when deleting records.
- Clear previous reports when starting a full repository check; preserve other repositories during scoped checks. Clear finished records within the current tab while retaining active batches and unresolved failures.
- Move GitHub imports to the background when the Add Skills dialog loses focus. Continue name-conflict resolution from Tasks, retry failures, cancel downloads and preparation, and let atomic commits finish safely. Allow stopping orphaned tasks and deleting stopping records without releasing active-write locks early.
- Speed up large-repository imports and updates by reusing bounded immutable snapshots, importing selected skills together at the previewed commit, resolving revisions once per update batch, revalidating only target skills, and refreshing lists once after a batch.
- Recognize root skills moved into repository subdirectories, fixing false source-deleted results such as last30days. Successful checks refresh displayed check times without changing skill files.
- Put the persisted grouped/flat view switch after Open directory in every skill toolbar. Add a sortable Repository column and owner/repository or repository@owner formats in flat view. Add configurable F3 view switching and F5 refresh; retain expansion shortcuts without the expand-all header icon.
- Preserve each view's scroll position, align sidebar and workspace headers/footers, and improve text contrast. Remove extra backgrounds from disabled inputs, tags, sidebar counts and inspector metadata; selected tags use the theme highlight color.
- Refresh bilingual usage documentation and screenshots for grouped/flat browsing, imports, bundles, update actions and both Tasks tabs, using fictional data.

## 1.1.1 - 2026-09-23

- Add an Update skills confirmation dialog with Last results, Check updates, Update stars and Cancel. Refresh repository stars independently, with progress and retries in background tasks.
- Show Update status and check timestamps in the status bar. Move Delete & reimport to the footer and apply selected replacements separately from regular updates.
- Toggle repository names between owner/repository and repository@owner, sorting by the displayed name. Place format and expand/collapse controls on the right of the name header.
- Append and deduplicate AI tag suggestions while preserving existing tags. Fix notes and tags save/clear buttons changing together.
- Dismiss search scope filters on focus loss, outside clicks or Escape.
- Unify common action icons and show platform-specific icons in Settings. Refine background task layout and manual stop, retry and cleanup controls.
- Preserve repository stars and cache timestamps in local and WebDAV backups, with support for older backups. Add star counts to directory CSV exports.

## 1.1.0 - 2026-09-22

- Unify GitHub and local imports, simplify the import wizard, and distinguish add, update and refresh with icon-only buttons and tooltips.
- Add collapsible repositories, status filters, a resizable update dialog, selected-repository checks and footer retries for failed checks; serialize checks and writes within each repository.
- Detect overwrite conflicts by the actual destination, avoiding false conflicts for matching IDs in different repositories; preserve destination IDs on reimport.
- Validate content versions before applying updates and accept retries of completed additions or updates. Group replacement candidates under remote deletions with a Delete and reimport action.
- Add tag renaming, deletion, batch drag-and-drop assignment, filter clearing, notes editing and configurable search scopes.
- Add custom platform icons, sidebar management and counts, plus context-sensitive status statistics.
- Add Ctrl+F search focus and Esc to clear and leave search; refresh bilingual documentation and feature screenshots.

## 1.0.0 - 2026-09-18

- Release Arsenal 1.0.0 with a skill resource library, collections, a central skills directory, and installation management for platforms and projects.
- Support skill imports, updates from source repositories, AI explanations, and backups.
- Improve Git ignore rules to exclude build caches, local application data, and key files.
