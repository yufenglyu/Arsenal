//! Logical ordered tables. Metadata uses bounded copy-on-write hash buckets;
//! callers and logical backups still see one sorted JSON map.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, ops::Index, sync::Arc};

pub(super) type Rows = BTreeMap<String, Value>;
pub(super) const SHARDED_TABLE: &str = "skill_metadata";
pub(super) const SHARD_THRESHOLD: usize = 8192;

#[derive(Clone, Debug, Default)]
pub struct Table {
    flat: Rows,
    pub(super) sharded: bool,
    pub(super) buckets: BTreeMap<u8, Arc<Rows>>,
    pub(super) refs: BTreeMap<u8, String>,
}

pub(super) fn bucket(key: &str) -> u8 {
    Sha256::digest(key.as_bytes())[0]
}

pub enum TableIter<'a> {
    Flat(std::collections::btree_map::Iter<'a, String, Value>),
    Sharded(std::vec::IntoIter<(&'a String, &'a Value)>),
}
impl<'a> Iterator for TableIter<'a> {
    type Item = (&'a String, &'a Value);
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Flat(iter) => iter.next(),
            Self::Sharded(iter) => iter.next(),
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Flat(iter) => iter.size_hint(),
            Self::Sharded(iter) => iter.size_hint(),
        }
    }
}
impl DoubleEndedIterator for TableIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::Flat(iter) => iter.next_back(),
            Self::Sharded(iter) => iter.next_back(),
        }
    }
}
impl ExactSizeIterator for TableIter<'_> {}

impl Table {
    pub(super) fn make_sharded(&mut self) {
        if self.sharded {
            return;
        }
        self.sharded = true;
        for (key, value) in std::mem::take(&mut self.flat) {
            self.insert(key, value);
        }
    }

    fn rows_mut(&mut self, key: &str) -> &mut Rows {
        if !self.sharded {
            return &mut self.flat;
        }
        let id = bucket(key);
        self.refs.remove(&id);
        Arc::make_mut(self.buckets.entry(id).or_default())
    }

    pub fn insert(&mut self, key: String, value: Value) -> Option<Value> {
        self.rows_mut(&key).insert(key, value)
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        if self.sharded {
            self.buckets.get(&bucket(key))?.get(key)
        } else {
            self.flat.get(key)
        }
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.rows_mut(key).get_mut(key)
    }
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.rows_mut(key).remove(key)
    }
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    pub fn len(&self) -> usize {
        if self.sharded {
            self.buckets.values().map(|rows| rows.len()).sum()
        } else {
            self.flat.len()
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn clear(&mut self) {
        self.flat.clear();
        self.buckets.clear();
        self.refs.clear();
    }
    pub fn entry(&mut self, key: String) -> std::collections::btree_map::Entry<'_, String, Value> {
        self.rows_mut(&key).entry(key)
    }
    pub fn retain(&mut self, mut f: impl FnMut(&String, &mut Value) -> bool) {
        if self.sharded {
            let keys: Vec<_> = self.keys().cloned().collect();
            for key in keys {
                if !f(&key, self.get_mut(&key).unwrap()) {
                    self.remove(&key);
                }
            }
        } else {
            self.flat.retain(f);
        }
    }
    pub fn iter(&self) -> TableIter<'_> {
        if !self.sharded {
            return TableIter::Flat(self.flat.iter());
        }
        let mut rows: Vec<_> = self.buckets.values().flat_map(|rows| rows.iter()).collect();
        rows.sort_unstable_by(|a, b| a.0.cmp(b.0));
        TableIter::Sharded(rows.into_iter())
    }
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &Value> {
        self.iter().map(|(_, v)| v)
    }
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &String> {
        self.iter().map(|(k, _)| k)
    }
    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = (&String, &mut Value)> + '_> {
        if !self.sharded {
            return Box::new(self.flat.iter_mut());
        }
        self.refs.clear();
        let mut rows: Vec<_> = self
            .buckets
            .values_mut()
            .flat_map(|rows| Arc::make_mut(rows).iter_mut())
            .collect();
        rows.sort_unstable_by(|a, b| a.0.cmp(b.0));
        Box::new(rows.into_iter())
    }
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.iter_mut().map(|(_, v)| v)
    }
    pub(super) fn into_rows(self) -> Rows {
        debug_assert!(!self.sharded);
        self.flat
    }
}

impl<'a> IntoIterator for &'a Table {
    type Item = (&'a String, &'a Value);
    type IntoIter = TableIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a> IntoIterator for &'a mut Table {
    type Item = (&'a String, &'a mut Value);
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
impl Index<&str> for Table {
    type Output = Value;
    fn index(&self, key: &str) -> &Value {
        self.get(key).expect("missing table key")
    }
}
impl Serialize for Table {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_map(self.iter())
    }
}
impl<'de> Deserialize<'de> for Table {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self {
            flat: Rows::deserialize(deserializer)?,
            ..Self::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sharded_map_preserves_order_mutations_snapshots_and_logical_json() {
        let mut table = Table::default();
        table.make_sharded();
        let mut expected = Rows::new();
        for i in 0..2048 {
            let key = format!("key-{}", (i * 37) % 2048);
            table.insert(key.clone(), json!(i));
            expected.insert(key, json!(i));
        }
        let snapshot = table.clone();
        let snapshot_json = serde_json::to_vec(&snapshot).unwrap();
        for i in 0..128 {
            let key = format!("key-{i}");
            *table.get_mut(&key).unwrap() = json!(i * 2);
            *expected.get_mut(&key).unwrap() = json!(i * 2);
        }
        for i in 100..200 {
            let key = format!("key-{i}");
            assert_eq!(table.remove(&key), expected.remove(&key));
        }
        table.entry("extra".into()).or_insert(json!(5));
        expected.entry("extra".into()).or_insert(json!(5));
        let mut visits = Vec::new();
        table.retain(|key, value| {
            visits.push(key.clone());
            *value = json!(value.as_i64().unwrap() + 1);
            !key.ends_with('3')
        });
        let mut expected_visits = Vec::new();
        expected.retain(|key, value| {
            expected_visits.push(key.clone());
            *value = json!(value.as_i64().unwrap() + 1);
            !key.ends_with('3')
        });
        assert_eq!(visits, expected_visits);
        assert_eq!(table.len(), expected.len());
        assert_eq!(
            serde_json::to_vec(&table).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        assert_eq!(serde_json::to_vec(&snapshot).unwrap(), snapshot_json);
        for value in table.values_mut() {
            *value = Value::Null;
        }
        for value in expected.values_mut() {
            *value = Value::Null;
        }
        assert_eq!(
            serde_json::to_vec(&table).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
        table.clear();
        assert!(table.is_empty());
        assert!(!snapshot.is_empty());
    }
    #[test]
    fn metadata_threshold_does_not_split_small_or_oscillate_existing_tables() {
        use crate::storage::store::Data;
        let mut data = Data::default();
        for i in 0..SHARD_THRESHOLD {
            data.table_mut(SHARDED_TABLE)
                .insert(i.to_string(), json!(i));
        }
        assert!(!data.table(SHARDED_TABLE).sharded);
        let snapshot = data.clone();
        data.table_mut(SHARDED_TABLE)
            .insert("extra".into(), json!(0));
        assert!(data.table(SHARDED_TABLE).sharded);
        assert!(!snapshot.table(SHARDED_TABLE).sharded);
        assert_eq!(snapshot.table(SHARDED_TABLE).len(), SHARD_THRESHOLD);
        data.table_mut(SHARDED_TABLE).clear();
        data.table_mut(SHARDED_TABLE)
            .insert("small-again".into(), json!(1));
        assert!(data.table(SHARDED_TABLE).sharded);
        // Other tables retain the original single-object representation.
        for i in 0..SHARD_THRESHOLD + 1 {
            data.table_mut("skills").insert(i.to_string(), json!(i));
        }
        assert!(!data.table("skills").sharded);
    }
}
