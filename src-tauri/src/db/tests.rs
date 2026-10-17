#[cfg(test)]
mod tests {
    use crate::db::*;
    use crate::path_utils::resolve_home_dir;
    use tempfile::tempdir;

    /// Create an in-memory SQLite pool and initialize the schema.
    async fn setup_test_db() -> AppStore {
        let pool = AppStore::connect(":memory:")
            .await
            .expect("Failed to create in-memory SQLite pool");
        init_database(&pool)
            .await
            .expect("Failed to initialize test database");
        pool
    }

    #[tokio::test]
    async fn successful_check_preserves_installed_sync_baseline() {
        let pool = setup_test_db().await;
        upsert_skill_source(
            &pool,
            &SkillSource {
                skill_id: "check-only".into(),
                source_type: "github".into(),
                source_url: None,
                source_author: None,
                source_repo: Some("example/repo".into()),
                source_path: None,
                updated_at: "2026-01-01T00:00:00Z".into(),
            },
        )
        .await
        .unwrap();
        let baseline = SkillSourceSync {
            skill_id: "check-only".into(),
            sync_scope: "github-repository".into(),
            remote_ref: Some("installed-commit".into()),
            skill_fingerprint: Some("installed-content".into()),
            last_checked_at: None,
            last_sync_at: Some("2026-01-01T00:00:00Z".into()),
            sync_status: "success".into(),
            sync_error: None,
            remote_deleted: false,
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        upsert_skill_source_sync(&pool, &baseline).await.unwrap();
        let time = "2026-10-08T08:00:00Z";
        record_skill_check_times(&pool, std::slice::from_ref(&baseline.skill_id), time)
            .await
            .unwrap();
        let result = get_skill_source_sync(&pool, &baseline.skill_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.last_checked_at.as_deref(), Some(time));
        assert_eq!(result.remote_ref, baseline.remote_ref);
        assert_eq!(result.skill_fingerprint, baseline.skill_fingerprint);
        assert_eq!(result.last_sync_at, baseline.last_sync_at);
        assert_eq!(result.sync_status, baseline.sync_status);
    }

    #[tokio::test]
    async fn prepare_config_dir_writes_platform_files_and_empty_schema() {
        let dir = tempdir().expect("temp dir");
        prepare_config_dir(dir.path()).await.unwrap();

        assert!(!dir.path().join("platform/platform.json").exists());
        assert!(dir.path().join("config.json").exists());
        assert!(!dir.path().join("platform").exists());
        assert!(!dir.path().join("library").exists());
        assert!(!dir.path().join("db.sqlite").exists());
        assert!(dir.path().join("store/manifest.json").exists());
        assert!(dir.path().join("icons/cursor.svg").exists());
        assert!(dir.path().join("icons/SOURCES.md").exists());

        let catalog = std::fs::read_to_string(dir.path().join("config.json")).unwrap();
        assert!(catalog.contains("~/.claude/skills"));
        assert!(catalog.contains("\"enabled\": true"));

        let pool = open_test_store(&path_to_string(&dir.path().join("db.sqlite")))
            .await
            .unwrap();
        let agent_count = pool.read(|d| Ok(d.table("agents").len())).unwrap();
        assert_eq!(
            agent_count, 0,
            "packaged databases must not bake builder home paths into agent rows"
        );
    }

    #[tokio::test]
    async fn github_stars_cache_is_case_insensitive_and_keeps_zero() {
        let pool = setup_test_db().await;
        assert_eq!(get_github_stars(&pool, "Example/Repo").await.unwrap(), None);
        save_github_stars(&pool, "Example/Repo", 1200)
            .await
            .unwrap();
        assert_eq!(
            get_github_stars(&pool, "example/repo").await.unwrap(),
            Some(1200)
        );
        save_github_stars(&pool, "EXAMPLE/REPO", 0).await.unwrap();
        assert_eq!(
            get_github_stars(&pool, "Example/Repo").await.unwrap(),
            Some(0)
        );
        let count = pool
            .read(|d| Ok(d.table("github_repository_stats").len()))
            .unwrap();
        assert_eq!(count, 1);
    }

    // ── Init ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_init_creates_all_tables() {
        let pool = setup_test_db().await;

        // Verify all core tables exist by counting rows (empty is fine)
        let tables = [
            "skills",
            "skill_installations",
            "skill_sources",
            "skill_source_syncs",
            "agent_skill_observations",
            "agents",
            "collections",
            "collection_skills",
        ];
        for table in &tables {
            let result = pool.read(|d| Ok(d.table(table).len()));
            assert!(result.is_ok(), "Table '{}' should exist", table);
        }
    }

    #[tokio::test]
    async fn test_init_is_idempotent() {
        let pool = setup_test_db().await;
        // Calling init_database again should not fail
        let result = init_database(&pool).await;
        assert!(result.is_ok(), "Second init should be idempotent");
    }

    #[tokio::test]
    async fn test_upsert_and_get_skill_source_sync() {
        let pool = setup_test_db().await;
        let now = "2026-08-13T10:00:00Z".to_string();
        upsert_skill_source(
            &pool,
            &SkillSource {
                skill_id: "ask-matt".to_string(),
                source_type: "github".to_string(),
                source_url: Some("mattpocock/skills".to_string()),
                source_author: Some("skills@1.0.0".to_string()),
                source_repo: Some("mattpocock/skills".to_string()),
                source_path: Some("ask-matt".to_string()),
                updated_at: now.clone(),
            },
        )
        .await
        .unwrap();
        let sync = SkillSourceSync {
            skill_id: "ask-matt".to_string(),
            sync_scope: "github-repository".to_string(),
            remote_ref: Some("abc123".to_string()),
            skill_fingerprint: Some("hash-a".to_string()),
            last_checked_at: Some(now.clone()),
            last_sync_at: Some(now.clone()),
            sync_status: "success".to_string(),
            sync_error: None,
            remote_deleted: false,
            updated_at: now.clone(),
        };

        upsert_skill_source_sync(&pool, &sync).await.unwrap();
        let stored = get_skill_source_sync(&pool, "ask-matt")
            .await
            .unwrap()
            .expect("sync row");
        assert_eq!(stored.sync_scope, "github-repository");
        assert_eq!(stored.remote_ref.as_deref(), Some("abc123"));
        assert!(!stored.remote_deleted);

        upsert_skill_source_sync(
            &pool,
            &SkillSourceSync {
                sync_scope: "skill".to_string(),
                remote_ref: Some("def456".to_string()),
                sync_status: "unchanged".to_string(),
                remote_deleted: true,
                ..sync
            },
        )
        .await
        .unwrap();
        let updated = get_skill_source_sync(&pool, "ask-matt")
            .await
            .unwrap()
            .expect("updated sync row");
        assert_eq!(updated.sync_scope, "skill");
        assert_eq!(updated.remote_ref.as_deref(), Some("def456"));
        assert_eq!(updated.sync_status, "unchanged");
        assert!(updated.remote_deleted);
    }

    #[tokio::test]
    async fn test_builtin_agents_seeded() {
        let pool = setup_test_db().await;
        let agents = get_all_agents(&pool).await.unwrap();
        assert_eq!(
            agents.len(),
            builtin_agents().len(),
            "Should have exactly the configured built-in agents"
        );

        let ids: Vec<&str> = agents.iter().map(|a| a.id.as_str()).collect();
        // Coding platforms
        assert!(ids.contains(&"claude-code"));
        assert!(ids.contains(&"codex"));
        assert!(ids.contains(&"cursor"));
        assert!(ids.contains(&"antigravity"));
        assert!(ids.contains(&"cline"));
        assert!(!ids.contains(&"deep-agents"));
        assert!(!ids.contains(&"dexto"));
        assert!(!ids.contains(&"firebender"));
        assert!(ids.contains(&"gemini-cli"));
        assert!(ids.contains(&"kimi-code-cli"));
        assert!(ids.contains(&"trae"));
        assert!(!ids.contains(&"factory-droid"));
        assert!(!ids.contains(&"junie"));
        assert!(ids.contains(&"qwen"));
        assert!(ids.contains(&"trae-cn"));
        assert!(ids.contains(&"windsurf"));
        assert!(ids.contains(&"qoder"));
        assert!(!ids.contains(&"augment"));
        assert!(ids.contains(&"opencode"));
        assert!(!ids.contains(&"kilocode"));
        assert!(!ids.contains(&"ob1"));
        assert!(!ids.contains(&"amp"));
        assert!(ids.contains(&"kiro"));
        assert!(ids.contains(&"codebuddy"));
        assert!(!ids.contains(&"aider-desk"));
        assert!(!ids.contains(&"bob"));
        assert!(ids.contains(&"codearts-agent"));
        assert!(!ids.contains(&"codemaker"));
        assert!(!ids.contains(&"codestudio"));
        assert!(!ids.contains(&"command-code"));
        assert!(!ids.contains(&"continue"));
        assert!(!ids.contains(&"cortex"));
        assert!(!ids.contains(&"crush"));
        assert!(!ids.contains(&"devin"));
        assert!(!ids.contains(&"forgecode"));
        assert!(!ids.contains(&"goose"));
        assert!(!ids.contains(&"iflow-cli"));
        assert!(!ids.contains(&"kode"));
        assert!(!ids.contains(&"mcpjam"));
        assert!(!ids.contains(&"mistral-vibe"));
        assert!(!ids.contains(&"mux"));
        assert!(!ids.contains(&"openhands"));
        assert!(ids.contains(&"pi"));
        assert!(!ids.contains(&"rovodev"));
        assert!(!ids.contains(&"roo"));
        assert!(!ids.contains(&"tabnine-cli"));
        assert!(!ids.contains(&"zencoder"));
        assert!(!ids.contains(&"neovate"));
        assert!(!ids.contains(&"pochi"));
        assert!(!ids.contains(&"adal"));
        assert!(ids.contains(&"hermes"));
        assert!(ids.contains(&"copilot"));
        assert!(ids.contains(&"warp"));
        assert!(!ids.contains(&"aider"));
        // Additional supported platforms
        assert!(ids.contains(&"openclaw"));
        assert!(!ids.contains(&"qclaw"));
        assert!(!ids.contains(&"easyclaw"));
        assert!(!ids.contains(&"autoclaw"));
        assert!(ids.contains(&"workbuddy"));
        // Central
        assert!(ids.contains(&"central"));
    }

    #[test]
    fn test_builtin_agents_have_expected_retained_platform_skill_dirs() {
        let home = resolve_home_dir();
        let agents_by_id: std::collections::HashMap<String, Agent> = builtin_agents()
            .into_iter()
            .map(|agent| (agent.id.clone(), agent))
            .collect();

        let expected = [
            (
                "claude-code",
                "Claude Code",
                ".claude/skills",
                ".claude/skills",
            ),
            ("openclaw", "OpenClaw", ".openclaw/skills", "skills"),
            (
                "codearts-agent",
                "CodeArts Agent",
                ".codeartsdoer/skills",
                ".codeartsdoer/skills",
            ),
            (
                "codebuddy",
                "CodeBuddy",
                ".codebuddy/skills",
                ".codebuddy/skills",
            ),
            ("kiro", "Kiro CLI", ".kiro/skills", ".kiro/skills"),
            ("pi", "Pi", ".pi/agent/skills", ".pi/skills"),
            ("qoder", "Qoder", ".qoder/skills", ".qoder/skills"),
            ("qwen", "Qwen Code", ".qwen/skills", ".qwen/skills"),
            ("trae", "Trae", ".trae/skills", ".trae/skills"),
            ("trae-cn", "Trae CN", ".trae-cn/skills", ".trae/skills"),
            (
                "windsurf",
                "Windsurf",
                ".codeium/windsurf/skills",
                ".windsurf/skills",
            ),
        ];

        for (id, display_name, global_rel, project_rel) in expected {
            let agent = agents_by_id
                .get(id)
                .unwrap_or_else(|| panic!("missing built-in agent {id}"));
            assert_eq!(agent.display_name, display_name, "display_name for {id}");
            assert_eq!(
                agent.global_skills_dir,
                path_to_string(&home.join(global_rel)),
                "global_skills_dir for {id}"
            );
            assert_eq!(
                agent.project_skills_dir.as_deref(),
                Some(project_rel),
                "project_skills_dir for {id}"
            );
        }

        assert!(!agents_by_id.contains_key("droid"));
        assert!(!agents_by_id.contains_key("qwen-code"));
        assert!(!agents_by_id.contains_key("kilo"));
        assert!(!agents_by_id.contains_key("kiro-cli"));
    }

    #[test]
    fn test_codex_uses_private_skills_dir_separate_from_central() {
        let home = resolve_home_dir();
        let agents_by_id: std::collections::HashMap<String, Agent> = builtin_agents()
            .into_iter()
            .map(|agent| (agent.id.clone(), agent))
            .collect();

        let codex = agents_by_id.get("codex").expect("missing Codex agent");
        let central = agents_by_id.get("central").expect("missing central agent");

        assert_eq!(
            codex.global_skills_dir,
            path_to_string(&home.join(".codex/skills"))
        );
        assert_ne!(
            codex.global_skills_dir, central.global_skills_dir,
            "installing to Codex must not write into the central skills directory"
        );
        assert_eq!(codex.project_skills_dir.as_deref(), Some(".agents/skills"));
        assert!(agent_supports_universal_agents_skills("codex"));
    }

    #[tokio::test]
    async fn test_builtin_agents_are_marked_builtin() {
        let pool = setup_test_db().await;
        let agents = get_all_agents(&pool).await.unwrap();
        for agent in &agents {
            assert!(agent.is_builtin, "All seeded agents should be builtin");
        }
    }

    #[tokio::test]
    async fn test_init_does_not_duplicate_agents_on_reinit() {
        let pool = setup_test_db().await;
        init_database(&pool).await.unwrap(); // Call a second time
        let agents = get_all_agents(&pool).await.unwrap();
        assert_eq!(
            agents.len(),
            builtin_agents().len(),
            "Reinit must not duplicate agents"
        );
    }

    // ── Skills ────────────────────────────────────────────────────────────────

    fn make_skill(id: &str, name: &str, is_central: bool) -> Skill {
        Skill {
            id: id.to_string(),
            name: name.to_string(),
            description: Some(format!("Description for {}", name)),
            file_path: format!("/tmp/{}/SKILL.md", id),
            canonical_path: if is_central {
                Some(format!("/tmp/.agents/skills/{}", id))
            } else {
                None
            },
            is_central,
            source: None,
            content: Some("# Test Skill\n\nContent here.".to_string()),
            scanned_at: Utc::now().to_rfc3339(),
        }
    }

    #[tokio::test]
    async fn test_upsert_skill_insert() {
        let pool = setup_test_db().await;
        let skill = make_skill("test-skill", "Test Skill", false);
        upsert_skill(&pool, &skill).await.unwrap();

        let retrieved = get_skill_by_id(&pool, "test-skill").await.unwrap();
        assert!(retrieved.is_some());
        let s = retrieved.unwrap();
        assert_eq!(s.name, "Test Skill");
        assert!(!s.is_central);
    }

    #[tokio::test]
    async fn test_upsert_skill_update() {
        let pool = setup_test_db().await;
        let mut skill = make_skill("skill-1", "Original Name", false);
        upsert_skill(&pool, &skill).await.unwrap();

        skill.name = "Updated Name".to_string();
        upsert_skill(&pool, &skill).await.unwrap();

        let retrieved = get_skill_by_id(&pool, "skill-1").await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated Name");
    }

    #[tokio::test]
    async fn test_upsert_skill_noncentral_scan_does_not_overwrite_central_metadata() {
        let pool = setup_test_db().await;
        let central = make_skill("shared-skill", "Central Name", true);
        upsert_skill(&pool, &central).await.unwrap();

        let mut platform = make_skill("shared-skill", "Platform Name", false);
        platform.description = Some("Platform description".to_string());
        platform.file_path = "/tmp/.hermes/skills/category/shared-skill/SKILL.md".to_string();
        platform.source = Some("copy".to_string());
        platform.scanned_at = "2026-04-28T00:00:00Z".to_string();
        upsert_skill(&pool, &platform).await.unwrap();

        let retrieved = get_skill_by_id(&pool, "shared-skill")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.name, "Central Name");
        assert_eq!(retrieved.description, central.description);
        assert_eq!(retrieved.file_path, central.file_path);
        assert_eq!(retrieved.canonical_path, central.canonical_path);
        assert!(retrieved.is_central);
        assert_eq!(retrieved.scanned_at, "2026-04-28T00:00:00Z");
    }

    #[tokio::test]
    async fn test_get_skill_by_id_not_found() {
        let pool = setup_test_db().await;
        let result = get_skill_by_id(&pool, "nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_central_skills() {
        let pool = setup_test_db().await;
        upsert_skill(&pool, &make_skill("central-1", "Central One", true))
            .await
            .unwrap();
        upsert_skill(&pool, &make_skill("non-central", "Not Central", false))
            .await
            .unwrap();

        let central = get_central_skills(&pool).await.unwrap();
        assert_eq!(central.len(), 1);
        assert_eq!(central[0].id, "central-1");
    }

    #[tokio::test]
    async fn test_delete_skill() {
        let pool = setup_test_db().await;
        let skill = make_skill("to-delete", "Delete Me", false);
        upsert_skill(&pool, &skill).await.unwrap();

        delete_skill(&pool, "to-delete").await.unwrap();
        let result = get_skill_by_id(&pool, "to-delete").await.unwrap();
        assert!(result.is_none());
    }

    // ── Skill Installations ───────────────────────────────────────────────────

    fn make_installation(skill_id: &str, agent_id: &str, link_type: &str) -> SkillInstallation {
        SkillInstallation {
            skill_id: skill_id.to_string(),
            agent_id: agent_id.to_string(),
            installed_path: format!("/tmp/{}/{}", agent_id, skill_id),
            link_type: link_type.to_string(),
            symlink_target: if link_type == "symlink" {
                Some(format!("/tmp/.agents/skills/{}", skill_id))
            } else {
                None
            },
            created_at: Utc::now().to_rfc3339(),
        }
    }

    #[tokio::test]
    async fn test_upsert_and_get_skill_installation() {
        let pool = setup_test_db().await;
        let skill = make_skill("my-skill", "My Skill", false);
        upsert_skill(&pool, &skill).await.unwrap();

        let inst = make_installation("my-skill", "claude-code", "symlink");
        upsert_skill_installation(&pool, &inst).await.unwrap();

        let installations = get_skill_installations(&pool, "my-skill").await.unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "claude-code");
        assert_eq!(installations[0].link_type, "symlink");
    }

    #[tokio::test]
    async fn test_delete_skill_installation() {
        let pool = setup_test_db().await;
        let skill = make_skill("del-skill", "Del Skill", false);
        upsert_skill(&pool, &skill).await.unwrap();
        upsert_skill_installation(&pool, &make_installation("del-skill", "cursor", "copy"))
            .await
            .unwrap();

        delete_skill_installation(&pool, "del-skill", "cursor")
            .await
            .unwrap();

        let installations = get_skill_installations(&pool, "del-skill").await.unwrap();
        assert!(installations.is_empty());
    }

    #[tokio::test]
    async fn test_get_skills_by_agent() {
        let pool = setup_test_db().await;
        let skill_a = make_skill("skill-a", "Skill A", false);
        let skill_b = make_skill("skill-b", "Skill B", false);
        upsert_skill(&pool, &skill_a).await.unwrap();
        upsert_skill(&pool, &skill_b).await.unwrap();

        upsert_skill_installation(
            &pool,
            &make_installation("skill-a", "claude-code", "symlink"),
        )
        .await
        .unwrap();
        upsert_skill_installation(&pool, &make_installation("skill-b", "cursor", "copy"))
            .await
            .unwrap();

        let claude_skills = get_skills_by_agent(&pool, "claude-code").await.unwrap();
        assert_eq!(claude_skills.len(), 1);
        assert_eq!(claude_skills[0].id, "skill-a");

        let cursor_skills = get_skills_by_agent(&pool, "cursor").await.unwrap();
        assert_eq!(cursor_skills.len(), 1);
        assert_eq!(cursor_skills[0].id, "skill-b");

        let empty = get_skills_by_agent(&pool, "codex").await.unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn test_managed_installation_scanned_row_is_excluded_from_queries() {
        let pool = setup_test_db().await;
        upsert_skill(&pool, &make_skill("scanned-only", "Scanned Only", false))
            .await
            .unwrap();

        upsert_scanned_skill_installation(
            &pool,
            &make_installation("scanned-only", "cursor", "native"),
        )
        .await
        .unwrap();

        assert!(get_skill_installations(&pool, "scanned-only")
            .await
            .unwrap()
            .is_empty());
        assert!(get_skills_by_agent(&pool, "cursor")
            .await
            .unwrap()
            .is_empty());
        assert!(get_skills_for_agent(&pool, "cursor")
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            count_managed_skill_installations(&pool, "cursor")
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn test_managed_installation_application_upsert_upgrades_scanned_row() {
        let pool = setup_test_db().await;
        upsert_skill(&pool, &make_skill("upgraded", "Upgraded", false))
            .await
            .unwrap();
        let installation = make_installation("upgraded", "cursor", "native");

        upsert_scanned_skill_installation(&pool, &installation)
            .await
            .unwrap();
        upsert_skill_installation(&pool, &installation)
            .await
            .unwrap();

        let is_managed = pool
            .read(|d| {
                Ok(
                    d.table("skill_installations")[&key2("upgraded", "cursor")]["is_managed"]
                        .as_bool()
                        .unwrap() as i64,
                )
            })
            .unwrap();
        assert_eq!(is_managed, 1);
        assert_eq!(
            get_skill_installations(&pool, "upgraded")
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            count_managed_skill_installations(&pool, "cursor")
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn test_managed_installation_rescan_preserves_ownership_and_recorded_path() {
        let pool = setup_test_db().await;
        upsert_skill(&pool, &make_skill("re-scanned", "Re-scanned", false))
            .await
            .unwrap();
        let managed = make_installation("re-scanned", "cursor", "symlink");
        upsert_skill_installation(&pool, &managed).await.unwrap();

        let mut scanned = managed.clone();
        scanned.installed_path = "/tmp/cursor/re-scanned-updated".to_string();
        scanned.link_type = "native".to_string();
        scanned.symlink_target = None;
        upsert_scanned_skill_installation(&pool, &scanned)
            .await
            .unwrap();

        let row = pool
            .read(|d| Ok(d.table("skill_installations")[&key2("re-scanned", "cursor")].clone()))
            .unwrap();
        assert_eq!(row["is_managed"].as_bool().unwrap() as i64, 1);
        assert_eq!(
            row["installed_path"].as_str().unwrap(),
            managed.installed_path
        );
        assert_eq!(row["link_type"].as_str().unwrap(), managed.link_type);
        assert_eq!(
            row["symlink_target"].as_str().map(str::to_owned),
            managed.symlink_target
        );
        assert_eq!(
            count_managed_skill_installations(&pool, "cursor")
                .await
                .unwrap(),
            1
        );
    }

    // ── Agents ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_agent_by_id() {
        let pool = setup_test_db().await;
        let agent = get_agent_by_id(&pool, "claude-code").await.unwrap();
        assert!(agent.is_some());
        let a = agent.unwrap();
        assert_eq!(a.display_name, "Claude Code");
        assert_eq!(a.category, "platform");
        assert!(a.is_builtin);
    }

    #[tokio::test]
    async fn test_get_agent_by_id_not_found() {
        let pool = setup_test_db().await;
        let agent = get_agent_by_id(&pool, "nonexistent-agent").await.unwrap();
        assert!(agent.is_none());
    }

    #[tokio::test]
    async fn test_update_agent_detected() {
        let pool = setup_test_db().await;
        update_agent_detected(&pool, "cursor", true).await.unwrap();
        let agent = get_agent_by_id(&pool, "cursor").await.unwrap().unwrap();
        assert!(agent.is_detected);

        update_agent_detected(&pool, "cursor", false).await.unwrap();
        let agent = get_agent_by_id(&pool, "cursor").await.unwrap().unwrap();
        assert!(!agent.is_detected);
    }

    #[tokio::test]
    async fn test_insert_custom_agent() {
        let pool = setup_test_db().await;
        let custom = Agent {
            id: "my-custom-agent".to_string(),
            display_name: "My Custom Agent".to_string(),
            category: "platform".to_string(),
            global_skills_dir: "/tmp/custom/skills".to_string(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        insert_custom_agent(&pool, &custom).await.unwrap();

        let all = get_all_agents(&pool).await.unwrap();
        assert_eq!(
            all.len(),
            builtin_agents().len() + 1,
            "Should have builtins + 1 custom"
        );

        let retrieved = get_agent_by_id(&pool, "my-custom-agent")
            .await
            .unwrap()
            .unwrap();
        assert!(!retrieved.is_builtin);
        assert_eq!(retrieved.display_name, "My Custom Agent");
    }

    #[tokio::test]
    async fn test_delete_custom_agent() {
        let pool = setup_test_db().await;
        let custom = Agent {
            id: "deletable-agent".to_string(),
            display_name: "Deletable".to_string(),
            category: "platform".to_string(),
            global_skills_dir: "/tmp/deletable/skills".to_string(),
            project_skills_dir: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        };
        insert_custom_agent(&pool, &custom).await.unwrap();
        delete_custom_agent(&pool, "deletable-agent").await.unwrap();

        let retrieved = get_agent_by_id(&pool, "deletable-agent").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_can_delete_builtin_agent() {
        let pool = setup_test_db().await;
        let result = delete_custom_agent(&pool, "claude-code").await;
        assert!(result.is_ok(), "Built-in agents can be removed by the user");

        let retrieved = get_agent_by_id(&pool, "claude-code").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_builtin_agent_edit_is_applied_during_seed() {
        let pool = setup_test_db().await;
        let mut agents = builtin_agents();
        let claude = agents
            .iter_mut()
            .find(|agent| agent.id == "claude-code")
            .expect("claude-code");
        claude.display_name = "Edited Claude".to_string();
        claude.global_skills_dir = "/tmp/edited-claude/skills".to_string();

        seed_builtin_agents_from(&pool, &agents, true)
            .await
            .unwrap();

        let updated = get_agent_by_id(&pool, "claude-code")
            .await
            .unwrap()
            .expect("claude-code should still exist");
        assert_eq!(updated.display_name, "Edited Claude");
        assert_eq!(updated.global_skills_dir, "/tmp/edited-claude/skills");
        assert!(updated.is_builtin);
    }

    #[tokio::test]
    async fn test_removed_catalog_platform_is_dropped_during_seed() {
        let pool = setup_test_db().await;
        let agents: Vec<Agent> = builtin_agents()
            .into_iter()
            .filter(|agent| agent.id != "cursor")
            .collect();

        seed_builtin_agents_from(&pool, &agents, true)
            .await
            .unwrap();

        let deleted = get_agent_by_id(&pool, "cursor").await.unwrap();
        assert!(
            deleted.is_none(),
            "platforms missing from the catalog should be removed during seed"
        );
    }

    #[tokio::test]
    async fn test_workbuddy_scans_correct_directory() {
        let pool = setup_test_db().await;
        let wb = get_agent_by_id(&pool, "workbuddy")
            .await
            .unwrap()
            .expect("WorkBuddy agent should exist");
        assert_eq!(wb.display_name, "WorkBuddy");
        let workbuddy_path = wb.global_skills_dir.replace('\\', "/");
        assert!(
            workbuddy_path.contains(".workbuddy/skills"),
            "WorkBuddy should scan ~/.workbuddy/skills, got: {}",
            wb.global_skills_dir
        );
    }

    #[tokio::test]
    async fn test_trimmed_autoclaw_is_not_seeded_but_workbuddy_remains() {
        let pool = setup_test_db().await;
        assert!(get_agent_by_id(&pool, "autoclaw").await.unwrap().is_none());
        assert!(get_agent_by_id(&pool, "workbuddy").await.unwrap().is_some());
    }

    // ── Collections ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_collection() {
        let pool = setup_test_db().await;
        let col = create_collection(&pool, "My Collection", Some("A test collection"))
            .await
            .unwrap();
        assert!(!col.id.is_empty());
        assert_eq!(col.name, "My Collection");
        assert_eq!(col.description.as_deref(), Some("A test collection"));
    }

    #[tokio::test]
    async fn test_get_all_collections() {
        let pool = setup_test_db().await;
        create_collection(&pool, "Collection A", None)
            .await
            .unwrap();
        create_collection(&pool, "Collection B", Some("Desc"))
            .await
            .unwrap();

        let all = get_all_collections(&pool).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_update_collection() {
        let pool = setup_test_db().await;
        let col = create_collection(&pool, "Old Name", None).await.unwrap();
        update_collection(&pool, &col.id, "New Name", Some("New desc"))
            .await
            .unwrap();

        let updated = get_collection_by_id(&pool, &col.id).await.unwrap().unwrap();
        assert_eq!(updated.name, "New Name");
        assert_eq!(updated.description.as_deref(), Some("New desc"));
    }

    #[tokio::test]
    async fn test_delete_collection() {
        let pool = setup_test_db().await;
        let col = create_collection(&pool, "To Delete", None).await.unwrap();
        delete_collection(&pool, &col.id).await.unwrap();

        let retrieved = get_collection_by_id(&pool, &col.id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_add_and_remove_skill_from_collection() {
        let pool = setup_test_db().await;
        let skill = make_skill("collection-skill", "Collection Skill", false);
        upsert_skill(&pool, &skill).await.unwrap();
        let col = create_collection(&pool, "Test Col", None).await.unwrap();

        add_skill_to_collection(&pool, &col.id, "collection-skill")
            .await
            .unwrap();

        let skills = get_collection_skills(&pool, &col.id).await.unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "collection-skill");

        remove_skill_from_collection(&pool, &col.id, "collection-skill")
            .await
            .unwrap();

        let skills_after = get_collection_skills(&pool, &col.id).await.unwrap();
        assert!(skills_after.is_empty());
    }

    #[tokio::test]
    async fn test_add_skill_to_collection_is_idempotent() {
        let pool = setup_test_db().await;
        let skill = make_skill("idem-skill", "Idem Skill", false);
        upsert_skill(&pool, &skill).await.unwrap();
        let col = create_collection(&pool, "Idem Col", None).await.unwrap();

        add_skill_to_collection(&pool, &col.id, "idem-skill")
            .await
            .unwrap();
        add_skill_to_collection(&pool, &col.id, "idem-skill")
            .await
            .unwrap();

        let skills = get_collection_skills(&pool, &col.id).await.unwrap();
        assert_eq!(skills.len(), 1, "Duplicate add should be a no-op");
    }

    #[tokio::test]
    async fn test_delete_collection_also_removes_skill_memberships() {
        let pool = setup_test_db().await;
        let skill = make_skill("cascade-skill", "Cascade Skill", false);
        upsert_skill(&pool, &skill).await.unwrap();
        let col = create_collection(&pool, "Cascade Col", None).await.unwrap();
        add_skill_to_collection(&pool, &col.id, "cascade-skill")
            .await
            .unwrap();

        delete_collection(&pool, &col.id).await.unwrap();

        // The collection_skills row should also be gone
        let rows = collection_member_ids(&pool, &col.id).await.unwrap();
        assert!(rows.is_empty(), "Memberships should be cascade-deleted");
    }

    // ── Scan Directories ──────────────────────────────────────────────────────

    /// Returns the number of *unique* global_skills_dir paths across all
    /// built-in agents.  This is the number of rows that seed_builtin_scan_directories
    /// inserts.
    fn expected_builtin_scan_dir_count() -> usize {
        let mut paths = std::collections::HashSet::new();
        for agent in builtin_agents() {
            paths.insert(agent.global_skills_dir);
        }
        paths.len()
    }

    #[tokio::test]
    async fn test_builtin_scan_dirs_seeded() {
        let pool = setup_test_db().await;
        let dirs = get_scan_directories(&pool).await.unwrap();
        let builtin_count = expected_builtin_scan_dir_count();

        // Expect exactly one row per unique global_skills_dir across built-in agents.
        assert_eq!(
            dirs.len(),
            builtin_count,
            "Should have {} built-in scan directories after init (got {})",
            builtin_count,
            dirs.len()
        );

        // Every seeded row must be marked as built-in and active.
        for dir in &dirs {
            assert!(
                dir.is_builtin,
                "Seeded scan directory '{}' must have is_builtin=true",
                dir.path
            );
            assert!(
                dir.is_active,
                "Seeded scan directory '{}' must be active by default",
                dir.path
            );
        }

        // The paths must match the unique global_skills_dir values.
        let seeded_paths: std::collections::HashSet<&str> =
            dirs.iter().map(|d| d.path.as_str()).collect();
        for agent in builtin_agents() {
            assert!(
                seeded_paths.contains(agent.global_skills_dir.as_str()),
                "Built-in agent '{}' global_skills_dir '{}' must be in scan_directories",
                agent.id,
                agent.global_skills_dir
            );
        }
    }

    #[tokio::test]
    async fn test_builtin_scan_dirs_seeded_is_idempotent() {
        let pool = setup_test_db().await;
        // Second call to init_database must not create duplicate rows.
        init_database(&pool).await.unwrap();
        let dirs = get_scan_directories(&pool).await.unwrap();
        let builtin_count = expected_builtin_scan_dir_count();
        assert_eq!(
            dirs.len(),
            builtin_count,
            "Repeated init must not create duplicate scan directory rows"
        );
    }

    #[tokio::test]
    async fn test_add_scan_directory() {
        let pool = setup_test_db().await;
        let dir = add_scan_directory(&pool, "/tmp/my-project", Some("My Project"))
            .await
            .unwrap();
        assert_eq!(dir.path, "/tmp/my-project");
        assert_eq!(dir.label.as_deref(), Some("My Project"));
        assert!(dir.is_active);
        assert!(!dir.is_builtin);
    }

    #[tokio::test]
    async fn test_update_scan_directory_label_and_path() {
        let pool = setup_test_db().await;
        add_scan_directory(&pool, "/tmp/old-project", Some("Old"))
            .await
            .unwrap();
        let updated = update_scan_directory(
            &pool,
            "/tmp/old-project",
            "/tmp/new-project",
            Some("New Name"),
        )
        .await
        .unwrap();
        assert_eq!(updated.path, "/tmp/new-project");
        assert_eq!(updated.label.as_deref(), Some("New Name"));
    }

    #[tokio::test]
    async fn test_get_scan_directories() {
        let pool = setup_test_db().await;
        add_scan_directory(&pool, "/tmp/dir-a", None).await.unwrap();
        add_scan_directory(&pool, "/tmp/dir-b", Some("Dir B"))
            .await
            .unwrap();

        let dirs = get_scan_directories(&pool).await.unwrap();
        // There are N built-in dirs (seeded on init) plus the 2 we just added.
        let builtin_count = expected_builtin_scan_dir_count();
        assert_eq!(dirs.len(), builtin_count + 2);

        // Verify the custom ones are present.
        let paths: Vec<&str> = dirs.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"/tmp/dir-a"));
        assert!(paths.contains(&"/tmp/dir-b"));
    }

    #[tokio::test]
    async fn test_remove_scan_directory() {
        let pool = setup_test_db().await;
        add_scan_directory(&pool, "/tmp/removable", None)
            .await
            .unwrap();
        remove_scan_directory(&pool, "/tmp/removable")
            .await
            .unwrap();

        let dirs = get_scan_directories(&pool).await.unwrap();
        // Built-in dirs remain; only the custom one is removed.
        let builtin_count = expected_builtin_scan_dir_count();
        assert_eq!(dirs.len(), builtin_count);
        assert!(!dirs.iter().any(|d| d.path == "/tmp/removable"));
    }

    #[tokio::test]
    async fn test_cannot_remove_builtin_scan_directory() {
        let pool = setup_test_db().await;
        crate::db::upsert_builtin_scan_directory(&pool, "/builtin/path", "Builtin")
            .await
            .unwrap();

        let result = remove_scan_directory(&pool, "/builtin/path").await;
        assert!(
            result.is_err(),
            "Should not remove a builtin scan directory"
        );
    }

    #[tokio::test]
    async fn test_remove_nonexistent_scan_directory_returns_error() {
        let pool = setup_test_db().await;
        let result = remove_scan_directory(&pool, "/nonexistent/path").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_toggle_scan_directory() {
        let pool = setup_test_db().await;
        add_scan_directory(&pool, "/tmp/toggle-dir", None)
            .await
            .unwrap();
        toggle_scan_directory(&pool, "/tmp/toggle-dir", false)
            .await
            .unwrap();

        let dirs = get_scan_directories(&pool).await.unwrap();
        let dir = dirs.iter().find(|d| d.path == "/tmp/toggle-dir").unwrap();
        assert!(!dir.is_active);
    }

    // ── Settings ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_set_and_get_setting() {
        let pool = setup_test_db().await;
        set_setting(&pool, "theme", "dark").await.unwrap();
        let value = get_setting(&pool, "theme").await.unwrap();
        assert_eq!(value.as_deref(), Some("dark"));
    }

    #[tokio::test]
    async fn test_get_missing_setting_returns_none() {
        let pool = setup_test_db().await;
        let value = get_setting(&pool, "nonexistent_key").await.unwrap();
        assert!(value.is_none());
    }

    #[tokio::test]
    async fn test_set_setting_upserts() {
        let pool = setup_test_db().await;
        set_setting(&pool, "lang", "en").await.unwrap();
        set_setting(&pool, "lang", "zh").await.unwrap();
        let value = get_setting(&pool, "lang").await.unwrap();
        assert_eq!(value.as_deref(), Some("zh"));
    }

    #[tokio::test]
    async fn test_multiple_settings() {
        let pool = setup_test_db().await;
        set_setting(&pool, "key1", "val1").await.unwrap();
        set_setting(&pool, "key2", "val2").await.unwrap();
        set_setting(&pool, "key3", "val3").await.unwrap();

        assert_eq!(
            get_setting(&pool, "key1").await.unwrap().as_deref(),
            Some("val1")
        );
        assert_eq!(
            get_setting(&pool, "key2").await.unwrap().as_deref(),
            Some("val2")
        );
        assert_eq!(
            get_setting(&pool, "key3").await.unwrap().as_deref(),
            Some("val3")
        );
    }

    // ── Migration: created_at ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_migration_adds_created_at_to_skill_installations() {
        let pool = AppStore::memory().unwrap();
        pool.write(|d|{d.table_mut("skill_installations").insert(key2("legacy-skill","claude-code"),serde_json::json!({"skill_id":"legacy-skill","agent_id":"claude-code","installed_path":"/tmp/legacy","link_type":"symlink","symlink_target":null}));Ok(())}).await.unwrap();
        init_database(&pool).await.unwrap();
        let row = pool
            .read(|d| {
                Ok(d.table("skill_installations")[&key2("legacy-skill", "claude-code")].clone())
            })
            .unwrap();
        assert!(!row["created_at"].as_str().unwrap().is_empty());
    }
    #[tokio::test]
    async fn test_managed_installation_migration_requires_matching_symlink_target() {
        let pool = AppStore::memory().unwrap();
        for (id, canonical, kind, target) in [
            (
                "proven-symlink",
                Some("/tmp/proven"),
                "symlink",
                Some("/tmp/proven"),
            ),
            (
                "external-symlink",
                Some("/tmp/external"),
                "symlink",
                Some("/tmp/outside"),
            ),
            ("legacy-copy", Some("/tmp/copy"), "copy", None),
            ("legacy-native", Some("/tmp/native"), "native", None),
            ("unproven-symlink", None, "symlink", Some("/tmp/unproven")),
        ] {
            let mut skill = make_skill(id, id, false);
            skill.canonical_path = canonical.map(str::to_owned);
            upsert_skill(&pool, &skill).await.unwrap();
            pool.write(|d|{d.table_mut("skill_installations").insert(key2(id,"cursor"),serde_json::json!({"skill_id":id,"agent_id":"cursor","installed_path":"/tmp/target","link_type":kind,"symlink_target":target,"created_at":"2024-01-01"}));Ok(())}).await.unwrap();
        }
        init_database(&pool).await.unwrap();
        init_database(&pool).await.unwrap();
        pool.read(|d| {
            assert_eq!(d.table("skill_installations").len(), 5);
            for r in d.table("skill_installations").values() {
                assert_eq!(
                    r["is_managed"].as_bool(),
                    Some(r["skill_id"] == "proven-symlink")
                );
            }
            Ok(())
        })
        .unwrap();
    }
    #[tokio::test]
    async fn test_migration_skipped_when_created_at_already_exists() {
        let pool = setup_test_db().await;
        let i = make_installation("stable", "cursor", "copy");
        upsert_skill_installation(&pool, &i).await.unwrap();
        init_database(&pool).await.unwrap();
        assert_eq!(
            get_skill_installations(&pool, "stable").await.unwrap()[0].created_at,
            i.created_at
        );
    }
}

#[cfg(test)]
mod scan_batch_tests {
    use crate::db::*;
    #[tokio::test]
    async fn failed_scan_batch_is_atomic_and_keeps_managed_installation() {
        let pool=AppStore::memory().unwrap();
        let skill=Skill{id:"scan".into(),name:"Scan".into(),description:None,file_path:"/resource/SKILL.md".into(),canonical_path:Some("/resource".into()),is_central:true,source:None,content:None,scanned_at:"first".into()};
        let install=SkillInstallation{skill_id:skill.id.clone(),agent_id:"cursor".into(),installed_path:"/managed".into(),link_type:"symlink".into(),symlink_target:Some("/resource".into()),created_at:"first".into()};
        upsert_skill(&pool,&skill).await.unwrap();upsert_skill_installation(&pool,&install).await.unwrap();
        let mut scanned=skill.clone();scanned.name="Changed".into();scanned.is_central=false;scanned.scanned_at="next".into();
        let mut scanned_install=install.clone();scanned_install.installed_path="/other".into();
        pool.fail_next_commit();assert!(commit_scan_batch(&pool,&[scanned.clone()],&[scanned_install.clone()],&[]).await.is_err());
        assert_eq!(get_skill_by_id(&pool,"scan").await.unwrap().unwrap().scanned_at,"first");
        commit_scan_batch(&pool,&[scanned],&[scanned_install],&[]).await.unwrap();
        let saved=get_skill_by_id(&pool,"scan").await.unwrap().unwrap();assert_eq!(saved.name,"Scan");assert_eq!(saved.scanned_at,"next");
        assert_eq!(get_skill_installations(&pool,"scan").await.unwrap()[0].installed_path,"/managed");
    }
}
