//! Cheap immutable snapshots; only a session being edited is copied on write.
use super::Session;
use serde::{Deserialize, Serialize};
use std::{
    ops::{Index, IndexMut},
    sync::Arc,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionList(Vec<Arc<Session>>);

impl SessionList {
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn push(&mut self, session: Session) {
        self.0.push(Arc::new(session));
    }
    pub fn get(&self, index: usize) -> Option<&Session> {
        self.0.get(index).map(Arc::as_ref)
    }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Session> {
        self.0.get_mut(index).map(Arc::make_mut)
    }
    pub fn iter(&self) -> <&Self as IntoIterator>::IntoIter {
        self.0.iter().map(Arc::as_ref)
    }
    pub fn iter_mut(&mut self) -> <&mut Self as IntoIterator>::IntoIter {
        self.0.iter_mut().map(Arc::make_mut)
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&Session) -> bool) {
        self.0.retain(|s| keep(s));
    }
}
impl From<Vec<Session>> for SessionList {
    fn from(sessions: Vec<Session>) -> Self {
        Self(sessions.into_iter().map(Arc::new).collect())
    }
}
impl Index<usize> for SessionList {
    type Output = Session;
    fn index(&self, index: usize) -> &Session {
        &self.0[index]
    }
}
impl IndexMut<usize> for SessionList {
    fn index_mut(&mut self, index: usize) -> &mut Session {
        Arc::make_mut(&mut self.0[index])
    }
}
impl<'a> IntoIterator for &'a SessionList {
    type Item = &'a Session;
    type IntoIter =
        std::iter::Map<std::slice::Iter<'a, Arc<Session>>, fn(&'a Arc<Session>) -> &'a Session>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(Arc::as_ref)
    }
}
impl<'a> IntoIterator for &'a mut SessionList {
    type Item = &'a mut Session;
    type IntoIter = std::iter::Map<
        std::slice::IterMut<'a, Arc<Session>>,
        fn(&'a mut Arc<Session>) -> &'a mut Session,
    >;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut().map(Arc::make_mut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_share_history_and_isolate_edits_and_deletions() {
        let mut list: SessionList = vec![Session::new("a".into()), Session::new("b".into())].into();
        list[0].draft = "before".into();
        let snapshot = list.clone();
        assert!(std::ptr::eq(&list[0], &snapshot[0]));
        list[0].draft = "after".into();
        assert_eq!(snapshot[0].draft, "before");
        assert!(!std::ptr::eq(&list[0], &snapshot[0]));
        assert!(std::ptr::eq(&list[1], &snapshot[1]));
        list.retain(|s| s.project != std::path::Path::new("b"));
        assert_eq!(snapshot.len(), 2);
        let value = serde_json::to_value(&snapshot).unwrap();
        assert!(value.is_array());
        assert_eq!(
            serde_json::from_value::<SessionList>(value).unwrap()[0].draft,
            "before"
        );
    }
}
