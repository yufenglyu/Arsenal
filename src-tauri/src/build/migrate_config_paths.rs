fn main() {
    let result = (|| -> Result<(), String> {
        let root = std::env::args_os()
            .nth(1)
            .ok_or("Expected an explicit .arsenal directory")?;
        let root = std::path::absolute(root).map_err(|e| e.to_string())?;
        if root.file_name().and_then(|n| n.to_str()) != Some(".arsenal") || !root.is_dir() {
            return Err("Expected an existing .arsenal directory".into());
        }
        skills_manage_lib::config_dir_migration::migrate(&root)?;
        let store = skills_manage_lib::storage::store::Store::open(&root)?;
        let report = tauri::async_runtime::block_on(
            skills_manage_lib::config_path_migration::finish(&store),
        )?;
        println!(
            "Migrated paths: {}; retargeted links: {}; legacy alias removed: {}",
            report.paths, report.links, report.alias_removed
        );
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
