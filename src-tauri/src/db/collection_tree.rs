use super::*;
use std::collections::HashSet;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionChange {
    pub action: String,
    pub id: Option<String>,
    pub name: Option<String>,
    pub parent_id: Option<String>,
    pub before_id: Option<String>,
    pub skill_ids: Option<Vec<String>>,
    pub repositories: Option<Vec<String>>,
    pub source_collection_id: Option<String>,
    pub include_descendants: Option<bool>,
}

// Tree and membership changes commit together, including Shift-drag transfers.
pub async fn change(pool: &AppStore, request: CollectionChange) -> Result<Vec<Collection>, String> {
    pool.write(|data| {
        let mut items = rows::<Collection>(data, "collections")?;
        items.sort_by(|a,b| a.sort_order.cmp(&b.sort_order).then(a.created_at.cmp(&b.created_at)));
        let id = request.id.as_deref().unwrap_or("");
        let now = Utc::now().to_rfc3339();
        if request.action != "create" && !items.iter().any(|item| item.id == id) { return Err("Collection not found".into()); }
        match request.action.as_str() {
            "create" => items.push(Collection {id:Uuid::new_v4().to_string(), name:request.name.unwrap_or_default().trim().into(), parent_id:request.parent_id, sort_order:0, description:None, created_at:now.clone(), updated_at:now.clone()}),
            "rename" => { let item=items.iter_mut().find(|item|item.id==id).unwrap(); item.name=request.name.unwrap_or_default().trim().into(); item.updated_at=now.clone(); },
            "move" => {
                let index=items.iter().position(|item|item.id==id).unwrap();
                let mut item=items.remove(index); item.parent_id=request.parent_id; item.updated_at=now.clone();
                let position=if let Some(before)=request.before_id {items.iter().position(|other|other.id==before && other.parent_id==item.parent_id).ok_or("Invalid destination")?} else {items.len()};
                items.insert(position,item);
            },
            "delete" => {
                let mut removed=HashSet::from([id.to_owned()]);
                loop { let count=removed.len(); for item in &items {if item.parent_id.as_ref().is_some_and(|p|removed.contains(p)){removed.insert(item.id.clone());}} if count==removed.len(){break;} }
                items.retain(|item|!removed.contains(&item.id));
                data.table_mut("collection_skills").retain(|_,row|!removed.contains(field(row,"collection_id")));
            },
            "assign" | "transfer" | "detach" => {
                let mut ids=request.skill_ids.unwrap_or_default();
                // Resolve whole-repository drops against the current snapshot in this transaction.
                // Background sync may replace skill IDs while the browser still shows old rows.
                for repository in request.repositories.unwrap_or_default() {
                    let members:Vec<String>=data.table("skill_sources").values()
                        .filter(|row|field(row,"source_repo").eq_ignore_ascii_case(&repository))
                        .map(|row|field(row,"skill_id").to_owned())
                        .filter(|skill|data.table("skills").get(skill).is_some_and(|row|row.get("is_central").and_then(Value::as_bool)!=Some(true)))
                        .collect();
                    if members.is_empty(){return Err("Repository not found".into());}
                    ids.extend(members);
                }
                let mut seen=HashSet::new();
                ids.retain(|id|seen.insert(id.clone()));
                if ids.iter().any(|skill|!data.table("skills").contains_key(skill)) {return Err("Skill not found".into());}
                if request.action=="transfer" {
                    let source=request.source_collection_id.as_deref().ok_or("Source collection required")?;
                    if source==id || !items.iter().any(|item|item.id==source) {return Err("Invalid source collection".into());}
                    // Descendant-inclusive views may drag skills from any child of the source.
                    let mut sources=HashSet::from([source.to_owned()]);
                    if request.include_descendants.unwrap_or(true) {loop {let count=sources.len();for item in &items{if item.parent_id.as_ref().is_some_and(|p|sources.contains(p)){sources.insert(item.id.clone());}}if count==sources.len(){break;}}}
                    data.table_mut("collection_skills").retain(|_,row|!(sources.contains(field(row,"collection_id")) && ids.iter().any(|skill|skill==field(row,"skill_id"))));
                }
                let mut targets=HashSet::from([id.to_owned()]);
                if request.action=="detach" && request.include_descendants.unwrap_or(true) {loop {let count=targets.len();for item in &items{if item.parent_id.as_ref().is_some_and(|p|targets.contains(p)){targets.insert(item.id.clone());}}if count==targets.len(){break;}}}
                for skill in ids {
                    if request.action=="detach" {for target in &targets {data.table_mut("collection_skills").remove(&key2(target,&skill));}}
                    else {data.table_mut("collection_skills").entry(key2(id,&skill)).or_insert_with(||json!({"collection_id":id,"skill_id":skill,"added_at":now}));}
                }
                items.iter_mut().find(|item|item.id==id).unwrap().updated_at=now.clone();
                if let Some(source)=request.source_collection_id {if let Some(item)=items.iter_mut().find(|item|item.id==source){item.updated_at=now;}}
            },
            _ => return Err("Invalid collection action".into()),
        }
        for item in &items {
            if item.name.trim().is_empty() || item.name.chars().count()>100 {return Err("Invalid collection name".into());}
            let mut seen=HashSet::from([item.id.as_str()]); let mut parent=item.parent_id.as_deref();
            while let Some(id)=parent {if !seen.insert(id){return Err("Collection cycle".into());}parent=items.iter().find(|item|item.id==id).ok_or("Parent collection not found")?.parent_id.as_deref();}
        }
        data.table_mut("collections").retain(|key,_|items.iter().any(|item| &item.id==key));
        for (index,item) in items.iter_mut().enumerate(){item.sort_order=index as i64;put(data,"collections",item.id.clone(),item)?;}
        Ok(items)
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    fn req(value: Value) -> CollectionChange { serde_json::from_value(value).unwrap() }
    async fn setup() -> AppStore { let store=AppStore::connect(":memory:").await.unwrap(); crate::db::init_database(&store).await.unwrap();store }
    #[tokio::test]
    async fn repository_drop_resolves_members_after_background_replacement() {
        let store=setup().await;
        let group=create_collection(&store,"Target",None).await.unwrap();
        store.write(|d| {
            d.table_mut("skills").insert("current".into(),json!({"id":"current","is_central":false}));
            d.table_mut("skill_sources").insert("current".into(),json!({"skill_id":"current","source_repo":"example/research"}));
            Ok(())
        }).await.unwrap();
        assert!(change(&store,req(json!({"action":"assign","id":group.id,"skillIds":["old"]}))).await.is_err());
        change(&store,req(json!({"action":"assign","id":group.id,"repositories":["EXAMPLE/RESEARCH"],"skillIds":[]}))).await.unwrap();
        assert!(store.read(|d|Ok(d.table("collection_skills").contains_key(&key2(&group.id,"current")))).unwrap());
    }
    #[tokio::test]
    async fn missing_repository_rolls_back_and_mixed_members_are_deduplicated() {
        let store=setup().await;
        let group=create_collection(&store,"Target",None).await.unwrap();
        store.write(|d| {
            for (id,central) in [("member",false),("central",true)] {
                d.table_mut("skills").insert(id.into(),json!({"id":id,"is_central":central}));
                d.table_mut("skill_sources").insert(id.into(),json!({"skill_id":id,"source_repo":"example/tools"}));
            }
            Ok(())
        }).await.unwrap();
        assert!(change(&store,req(json!({"action":"assign","id":group.id,"skillIds":["member"],"repositories":["missing/repo"]}))).await.is_err());
        assert!(store.read(|d|Ok(d.table("collection_skills").is_empty())).unwrap());
        change(&store,req(json!({"action":"assign","id":group.id,"skillIds":["member"],"repositories":["example/tools","EXAMPLE/TOOLS"]}))).await.unwrap();
        assert_eq!(store.read(|d|Ok(d.table("collection_skills").len())).unwrap(),1);
    }
    #[tokio::test]
    async fn hierarchy_rejects_cycles_and_preserves_order_on_failure() {
        let store=setup().await;
        let first=change(&store,req(json!({"action":"create","name":"Parent"}))).await.unwrap()[0].clone();
        let items=change(&store,req(json!({"action":"create","name":"Child","parentId":first.id}))).await.unwrap();
        let child=items[1].clone();
        assert!(change(&store,req(json!({"action":"move","id":first.id,"parentId":child.id}))).await.is_err());
        assert!(get_collection_by_id(&store,&first.id).await.unwrap().unwrap().parent_id.is_none());
        change(&store,req(json!({"action":"move","id":child.id,"parentId":null,"beforeId":first.id}))).await.unwrap();
        assert_eq!(get_all_collections(&store).await.unwrap()[0].id,child.id);
    }
    #[tokio::test]
    async fn transfer_is_atomic_and_delete_only_removes_group_memberships() {
        let store=setup().await;
        let a=create_collection(&store,"A",None).await.unwrap();let b=create_collection(&store,"B",None).await.unwrap();
        store.write(|data|{data.table_mut("skills").insert("skill".into(),json!({"id":"skill"}));Ok(())}).await.unwrap();
        change(&store,req(json!({"action":"assign","id":a.id,"skillIds":["skill"]}))).await.unwrap();
        assert!(change(&store,req(json!({"action":"transfer","id":b.id,"sourceCollectionId":a.id,"skillIds":["skill","missing"]}))).await.is_err());
        assert!(store.read(|data|Ok(data.table("collection_skills").contains_key(&key2(&a.id,"skill")))).unwrap());
        change(&store,req(json!({"action":"transfer","id":b.id,"sourceCollectionId":a.id,"skillIds":["skill"]}))).await.unwrap();
        store.read(|data|{assert!(!data.table("collection_skills").contains_key(&key2(&a.id,"skill")));assert!(data.table("collection_skills").contains_key(&key2(&b.id,"skill")));Ok(())}).unwrap();
        change(&store,req(json!({"action":"move","id":b.id,"parentId":a.id}))).await.unwrap();
        delete_collection(&store,&a.id).await.unwrap();
        assert!(get_all_collections(&store).await.unwrap().is_empty());
        store.read(|data|{assert!(data.table("collection_skills").is_empty());assert!(data.table("skills").contains_key("skill"));Ok(())}).unwrap();
    }
    #[test]
    fn old_collections_deserialize_as_root_groups() {
        let item:Collection=serde_json::from_value(json!({"id":"old","name":"Old","description":null,"created_at":"","updated_at":""})).unwrap();
        assert!(item.parent_id.is_none());assert_eq!(item.sort_order,0);
    }
}
