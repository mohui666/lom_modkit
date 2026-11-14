//! A chapter map that records possible writes without examining document contents.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
    ops::{Deref, DerefMut},
};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Stories {
    values: BTreeMap<String, Value>,
    #[serde(skip)]
    changed: BTreeSet<String>,
    #[serde(skip, default = "all_changed")]
    all_changed: bool,
}

fn all_changed() -> bool {
    true
}

impl Clone for Stories {
    fn clone(&self) -> Self {
        self.values.clone().into()
    }
}

impl Stories {
    /// None means that a caller obtained unrestricted mutable access.
    /// Entries are only candidates: editors still compare their contents before
    /// recording history, so borrowing a chapter mutably is not itself an edit.
    pub fn take_changes(&mut self) -> Option<BTreeSet<String>> {
        let all = std::mem::take(&mut self.all_changed);
        let changed = std::mem::take(&mut self.changed);
        (!all).then_some(changed)
    }

    pub fn get_mut<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<&mut Value>
    where
        String: Borrow<Q>,
    {
        if !self.all_changed {
            if let Some((id, _)) = self.values.get_key_value(key) {
                self.changed.insert(id.clone());
            }
        }
        self.values.get_mut(key)
    }

    pub fn insert(&mut self, id: String, story: Value) -> Option<Value> {
        if !self.all_changed {
            self.changed.insert(id.clone());
        }
        self.values.insert(id, story)
    }

    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<Value>
    where
        String: Borrow<Q>,
    {
        if !self.all_changed {
            if let Some((id, _)) = self.values.get_key_value(key) {
                self.changed.insert(id.clone());
            }
        }
        self.values.remove(key)
    }
}

impl Deref for Stories {
    type Target = BTreeMap<String, Value>;
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl DerefMut for Stories {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.all_changed = true;
        self.changed.clear();
        &mut self.values
    }
}
impl PartialEq for Stories {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}
impl PartialEq<BTreeMap<String, Value>> for Stories {
    fn eq(&self, other: &BTreeMap<String, Value>) -> bool {
        self.values == *other
    }
}
impl PartialEq<Stories> for BTreeMap<String, Value> {
    fn eq(&self, other: &Stories) -> bool {
        *self == other.values
    }
}
impl From<BTreeMap<String, Value>> for Stories {
    fn from(values: BTreeMap<String, Value>) -> Self {
        Self {
            values,
            all_changed: true,
            ..Self::default()
        }
    }
}
impl FromIterator<(String, Value)> for Stories {
    fn from_iter<T: IntoIterator<Item = (String, Value)>>(iter: T) -> Self {
        BTreeMap::from_iter(iter).into()
    }
}
impl<'a> IntoIterator for &'a Stories {
    type Item = (&'a String, &'a Value);
    type IntoIter = std::collections::btree_map::Iter<'a, String, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}
impl<'a> IntoIterator for &'a mut Stories {
    type Item = (&'a String, &'a mut Value);
    type IntoIter = std::collections::btree_map::IterMut<'a, String, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.deref_mut().iter_mut()
    }
}
impl IntoIterator for Stories {
    type Item = (String, Value);
    type IntoIter = std::collections::btree_map::IntoIter<String, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}
