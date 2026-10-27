use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Resolve reply descendants once; a long, reverse-ordered chain stays linear.
pub fn thread_entries(
    entries: Vec<crate::network::HistoryEntry>,
    root: &MessageRef,
) -> Vec<crate::network::HistoryEntry> {
    use std::collections::{HashMap, HashSet};
    let key = |peer: &str, id: &str| format!("{peer}:{id}");
    let mut children = HashMap::<String, Vec<String>>::new();
    for entry in &entries {
        let event = &entry.event;
        if event["type"] != "message" {
            continue;
        }
        if let Some(reference) = event.get("reply_to").filter(|v| !v.is_null()) {
            children
                .entry(key(
                    reference["peer"].as_str().unwrap_or_default(),
                    reference["id"].as_str().unwrap_or_default(),
                ))
                .or_default()
                .push(key(
                    event["peer"].as_str().unwrap_or_default(),
                    event["id"].as_str().unwrap_or_default(),
                ));
        }
    }
    let mut stack = vec![key(&root.peer, &root.id.to_string())];
    let mut included = HashSet::new();
    while let Some(current) = stack.pop() {
        if !included.insert(current.clone()) {
            continue;
        }
        if let Some(next) = children.remove(&current) {
            stack.extend(next);
        }
    }
    let mut matches = entries
        .into_iter()
        .filter(|entry| {
            entry.event["type"] == "message"
                && included.contains(&key(
                    entry.event["peer"].as_str().unwrap_or_default(),
                    entry.event["id"].as_str().unwrap_or_default(),
                ))
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|entry| entry.timestamp);
    matches
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageRef {
    pub id: Uuid,
    pub peer: String,
}
impl MessageRef {
    pub fn validate(&self) -> Result<()> {
        let _: iroh::PublicKey = self.peer.parse()?;
        ensure!(!self.id.is_nil(), "Invalid reply reference");
        Ok(())
    }
}

#[derive(Serialize)]
pub struct SearchPage {
    pub entries: Vec<crate::network_exports::HistoryEntry>,
    pub total: usize,
    pub offset: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mention {
    pub peer: String,
    pub name: String,
}
pub fn contains_mention(text: &str, name: &str) -> bool {
    let token = format!("@{name}");
    text.match_indices(&token).any(|(index, _)| {
        let before = text[..index].chars().next_back();
        let after = text[index + token.len()..].chars().next();
        before.is_none_or(|c| c.is_whitespace() || "([{\"'".contains(c))
            && after.is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '#')
    })
}
impl Mention {
    pub fn validate(&self, text: &str) -> Result<()> {
        let _: iroh::PublicKey = self.peer.parse()?;
        ensure!(
            !self.name.is_empty()
                && self.name.chars().count() <= 64
                && !self.name.chars().any(char::is_control)
                && contains_mention(text, &self.name),
            "Invalid mention"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadMarker {
    pub room: Uuid,
    pub through: u64,
    pub ids: Vec<Uuid>,
}
impl ReadMarker {
    pub fn empty(room: Uuid) -> Self {
        Self {
            room,
            through: 0,
            ids: vec![],
        }
    }
    pub fn covers(&self, timestamp: u64, id: Uuid) -> bool {
        timestamp < self.through || (timestamp == self.through && self.ids.contains(&id))
    }
    pub fn advance(&mut self, through: u64, ids: Vec<Uuid>) {
        if through < self.through {
            return;
        }
        if through > self.through {
            self.through = through;
            self.ids.clear();
        }
        for id in ids {
            if !self.ids.contains(&id) {
                self.ids.push(id);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mention_boundaries_and_read_ties_are_explicit() {
        assert!(contains_mention("Hello @Alice Smith!", "Alice Smith"));
        assert!(!contains_mention("mail@Alice", "Alice"));
        assert!(!contains_mention("@AliceOther", "Alice"));
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let mut read = ReadMarker::empty(Uuid::new_v4());
        read.advance(10, vec![first]);
        assert!(read.covers(10, first));
        assert!(!read.covers(10, second));
        read.advance(10, vec![second]);
        assert!(read.covers(10, second));
        read.advance(9, vec![]);
        assert_eq!(read.through, 10);
    }
}
