pub mod commands;
pub mod config_store;
pub mod db;
mod db_config;
pub mod path_utils;
pub mod platform_icons;
pub mod platforms;
mod protocol;
pub mod storage;

#[cfg(test)]
mod storage_baseline;

use db::DbPool;
use std::fs;
use std::path::Path;
use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;

/// Application state shared across Tauri commands.
pub struct AppState {
    pub db: DbPool,
}

fn check_storage_migration(new_dir: &Path, legacy_dir: &Path) -> Result<(), String> {
    if !new_dir.join("store/manifest.json").is_file()
        && (new_dir.join("db.sqlite").exists() || legacy_dir.join("db.sqlite").exists())
    {
        return Err("STORAGE_MIGRATION_REQUIRED: see docs/JSON存储迁移操作指南.md".into());
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(commands::notebook::NotebookRuntime::default())
        .manage(protocol::ProtocolRuntime::default())
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            commands::notebook::queue_navigation(app, args);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let db_dir = path_utils::app_data_dir();
            check_storage_migration(&db_dir, &path_utils::legacy_app_data_dir())
                .map_err(std::io::Error::other)?;
            fs::create_dir_all(&db_dir)?;
            let pool = storage::store::Store::open(&db_dir).map_err(std::io::Error::other)?;
            tauri::async_runtime::block_on(db::init_database(&pool))
                .map_err(std::io::Error::other)?;

            tauri::async_runtime::block_on(async {
                commands::repository_address::recover(&pool).await?;
                commands::github_import::recover_import_transactions(&pool).await
            })
            .map_err(std::io::Error::other)?;
            app.manage(AppState { db: pool });
            #[cfg(all(windows, not(debug_assertions)))]
            if app.config().identifier == "com.iamzhihuix.skillsmanage" {
                if let Err(error) = protocol::register(app.handle()) {
                    protocol::remember_startup_error(app.handle(), error);
                }
            }
            if let Some(urls) = app.deep_link().get_current()? {
                commands::notebook::queue_navigation(
                    app.handle(),
                    urls.into_iter().map(|u| u.to_string()),
                );
            }
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                commands::notebook::queue_navigation(
                    &handle,
                    event.urls().into_iter().map(|u| u.to_string()),
                );
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::notebook_repository::ensure_repository_notebook,
            commands::notebook_repository::save_internal_notebook,
            commands::notebook_repository::get_notebook_settings,
            commands::notebook_repository::save_notebook_settings,
            commands::notebook_repository::list_repository_notebooks,
            commands::notebook::read_notebook,
            commands::notebook::inspect_notebook,
            commands::notebook::preview_notebook_properties,
            commands::notebook::bind_notebook,
            commands::notebook::save_notebook,
            commands::notebook::rename_notebook,
            commands::notebook::unlink_notebook,
            commands::notebook::clear_legacy_notebook,
            commands::notebook::open_obsidian,
            commands::notebook::notebook_asset,
            commands::notebook::watch_notebook,
            commands::notebook::take_notebook_navigation,
            protocol::register_skillshub_protocol,
            protocol::take_protocol_registration_error,
            commands::remote_sources::apply_repository_update_item,
            commands::repository_mirror::apply_repository_snapshot,
            commands::repository_address::change_repository_address,
            commands::repository_mirror::release_repository_snapshots,
            commands::github_import::refresh_repository_stars,
            commands::github_import::refresh_repository,
            commands::skills::preview_local_resource_skills,
            commands::metadata::get_folder_notes,
            commands::metadata::save_folder_note,
            commands::metadata::count_skill_tag,
            commands::metadata::change_skill_tag,
            commands::metadata::rename_skill_tag,
            // Scanner
            commands::scanner::scan_all_skills,
            // Agents
            commands::agents::get_agents,
            commands::agents::detect_agents,
            commands::agents::add_custom_agent,
            commands::agents::update_custom_agent,
            commands::agents::remove_custom_agent,
            commands::agents::set_agent_enabled,
            platform_icons::get_platform_icons,
            platform_icons::set_platform_icon,
            // Linker
            commands::linker::install_skill_to_agent,
            commands::linker::add_resource_skill_to_central,
            commands::linker::remove_resource_skill_from_central,
            commands::linker::uninstall_skill_from_agent,
            commands::linker::batch_install_to_agents,
            commands::linker::batch_install_resource_skill_to_agents,
            // Skills
            commands::skills::get_skills_by_agent,
            commands::skills::get_central_skills,
            commands::skills::get_resource_library_skills,
            commands::skills::add_local_resource_skills,
            commands::skills::create_manual_resource_skill,
            commands::skills::update_skill_metadata,
            commands::skills::update_resource_skill_source_metadata,
            commands::skills::get_central_skill_bundles,
            commands::skills::get_central_skill_bundle_detail,
            commands::skills::preview_delete_central_skill_bundle,
            commands::skills::delete_central_skill_bundle,
            commands::skills::preview_delete_resource_skill_bundle,
            commands::skills::delete_resource_skill_bundle,
            commands::skills::delete_central_skill,
            commands::skills::delete_resource_skill,
            commands::skills::get_skill_detail,
            commands::skills::read_skill_content,
            commands::skills::read_file_by_path,
            commands::skills::read_repository_readme,
            commands::skills::read_repository_documents,
            commands::skills::list_skill_directory,
            commands::skills::open_in_file_manager,
            commands::skills::export_resource_skill_directory_list,
            // Backup
            commands::backup::export_app_backup,
            commands::backup::export_app_backup_to_path,
            commands::backup::import_app_backup,
            commands::backup::list_webdav_backups,
            commands::backup::test_webdav_connection,
            commands::backup::upload_webdav_backup,
            commands::backup::download_webdav_backup,
            commands::backup::delete_webdav_backup,
            // Collections
            commands::collections::create_collection,
            commands::collections::get_collections,
            commands::collections::get_collection_detail,
            commands::collections::add_skill_to_collection,
            commands::collections::remove_skill_from_collection,
            commands::collections::delete_collection,
            commands::collections::update_collection,
            commands::collections::batch_install_collection,
            commands::collections::export_collection,
            commands::collections::import_collection,
            // Settings
            commands::settings::get_scan_directories,
            commands::settings::add_scan_directory,
            commands::settings::update_scan_directory,
            commands::settings::remove_scan_directory,
            commands::settings::set_scan_directory_active,
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::settings::update_central_skills_dir,
            commands::settings::get_skill_resource_library_dir,
            commands::settings::update_skill_resource_library_dir,
            commands::settings::get_app_data_dir,
            commands::settings::get_platform_dir,
            commands::settings::update_app_data_dir,
            commands::settings::check_app_update,
            commands::github_import::preview_github_repo_import,
            commands::github_import::import_github_repo_skills,
            commands::github_cancellation::cancel_github_operation,
            commands::github_import::import_github_repo_snapshot,
            commands::github_import::fetch_github_skill_markdown,
            // Source updates and AI explanations
            commands::remote_sources::update_source_backed_central_skills,
            commands::remote_sources::update_source_backed_central_skill,
            commands::remote_sources::update_source_backed_resource_skills,
            commands::remote_sources::update_source_backed_resource_skill,
            commands::remote_sources::preview_source_backed_resource_repository_updates,
            commands::remote_sources::sync_source_backed_resource_skills,
            commands::remote_sources::explain_skill,
            commands::remote_sources::generate_repository_note,
            commands::remote_sources::generate_skill_tags,
            commands::remote_sources::get_skill_explanation,
            commands::remote_sources::explain_skill_stream,
            commands::remote_sources::refresh_skill_explanation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::check_storage_migration;
    #[test]
    fn legacy_storage_requires_explicit_migration_without_copying() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("db.sqlite"), b"legacy fixture").unwrap();
        assert!(check_storage_migration(&new, &old)
            .unwrap_err()
            .starts_with("STORAGE_MIGRATION_REQUIRED"));
        assert!(!new.exists());
        assert_eq!(
            std::fs::read(old.join("db.sqlite")).unwrap(),
            b"legacy fixture"
        );
    }
    #[test]
    fn valid_json_location_does_not_copy_old_database() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(new.join("store")).unwrap();
        std::fs::write(old.join("db.sqlite"), b"legacy fixture").unwrap();
        std::fs::write(
            new.join("store/manifest.json"),
            b"validated separately by Store::open",
        )
        .unwrap();
        check_storage_migration(&new, &old).unwrap();
        assert!(!new.join("db.sqlite").exists());
    }
}
