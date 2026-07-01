use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::event::{EarthquakeEvent, Intensity};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DedupDecision {
    New,
    Updated(Vec<EventChange>),
    Duplicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventChange {
    SerialAdvanced,
    BecameFinal,
    BecameCancelled,
    IntensityIncreased,
}

#[derive(Debug, Default, Clone)]
pub struct DedupState {
    seen: HashMap<String, EventSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EventSnapshot {
    serial: Option<u32>,
    is_final: bool,
    is_cancelled: bool,
    max_intensity: Option<Intensity>,
    last_seen_epoch_seconds: u64,
}

#[derive(Debug, Error)]
pub enum StateError {
    #[error("failed to read dedup state {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse dedup state JSON {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to create dedup state directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize dedup state: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to write dedup state {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid dedup state intensity {value}: {source}")]
    InvalidIntensity {
        value: String,
        #[source]
        source: crate::event::ParseIntensityError,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedDedupState {
    version: u32,
    entries: Vec<PersistedEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedEntry {
    key: String,
    serial: Option<u32>,
    is_final: bool,
    is_cancelled: bool,
    max_intensity: Option<String>,
    last_seen_epoch_seconds: u64,
}

impl DedupState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_from_path(path: impl AsRef<Path>, max_entries: usize) -> Result<Self, StateError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::new());
        }

        let text = fs::read_to_string(path).map_err(|source| StateError::Read {
            path: path.to_owned(),
            source,
        })?;
        let persisted: PersistedDedupState =
            serde_json::from_str(&text).map_err(|source| StateError::Parse {
                path: path.to_owned(),
                source,
            })?;
        let mut state = Self::from_persisted(persisted)?;
        state.prune(max_entries);
        Ok(state)
    }

    pub fn save_to_path(
        &mut self,
        path: impl AsRef<Path>,
        max_entries: usize,
    ) -> Result<(), StateError> {
        self.prune(max_entries);

        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|source| StateError::CreateDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }

        let persisted = self.to_persisted();
        let text = serde_json::to_string_pretty(&persisted)?;
        fs::write(path, text).map_err(|source| StateError::Write {
            path: path.to_owned(),
            source,
        })
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    pub fn check(&self, event: &EarthquakeEvent) -> DedupDecision {
        let key = event.dedup_key();
        let Some(previous) = self.seen.get(&key) else {
            return DedupDecision::New;
        };

        let current = EventSnapshot::from(event);
        previous.compare(&current)
    }

    pub fn mark_processed(&mut self, event: &EarthquakeEvent) {
        self.seen
            .insert(event.dedup_key(), EventSnapshot::from(event));
    }

    pub fn should_process_and_mark(&mut self, event: &EarthquakeEvent) -> DedupDecision {
        let decision = self.check(event);
        if !matches!(decision, DedupDecision::Duplicate) {
            self.mark_processed(event);
        }
        decision
    }

    pub fn prune(&mut self, max_entries: usize) {
        if max_entries == 0 || self.seen.len() <= max_entries {
            return;
        }

        let mut entries: Vec<_> = self
            .seen
            .iter()
            .map(|(key, snapshot)| (key.clone(), snapshot.last_seen_epoch_seconds))
            .collect();
        entries.sort_by_key(|(_, last_seen)| *last_seen);

        let remove_count = self.seen.len() - max_entries;
        for (key, _) in entries.into_iter().take(remove_count) {
            self.seen.remove(&key);
        }
    }

    fn from_persisted(persisted: PersistedDedupState) -> Result<Self, StateError> {
        let mut seen = HashMap::new();
        for entry in persisted.entries {
            let snapshot = EventSnapshot::try_from(&entry)?;
            seen.insert(entry.key, snapshot);
        }

        Ok(Self { seen })
    }

    fn to_persisted(&self) -> PersistedDedupState {
        let mut entries: Vec<_> = self
            .seen
            .iter()
            .map(|(key, snapshot)| PersistedEntry::from_snapshot(key, snapshot))
            .collect();
        entries.sort_by(|left, right| {
            right
                .last_seen_epoch_seconds
                .cmp(&left.last_seen_epoch_seconds)
                .then_with(|| left.key.cmp(&right.key))
        });

        PersistedDedupState {
            version: 1,
            entries,
        }
    }
}

impl EventSnapshot {
    fn compare(&self, current: &Self) -> DedupDecision {
        let mut changes = Vec::new();

        if serial_advanced(self.serial, current.serial) {
            changes.push(EventChange::SerialAdvanced);
        }

        if !self.is_final && current.is_final {
            changes.push(EventChange::BecameFinal);
        }

        if !self.is_cancelled && current.is_cancelled {
            changes.push(EventChange::BecameCancelled);
        }

        if intensity_increased(self.max_intensity, current.max_intensity) {
            changes.push(EventChange::IntensityIncreased);
        }

        if changes.is_empty() {
            DedupDecision::Duplicate
        } else {
            DedupDecision::Updated(changes)
        }
    }
}

impl From<&EarthquakeEvent> for EventSnapshot {
    fn from(event: &EarthquakeEvent) -> Self {
        Self {
            serial: event.serial,
            is_final: event.is_final,
            is_cancelled: event.is_cancelled,
            max_intensity: event.max_intensity,
            last_seen_epoch_seconds: current_epoch_seconds(),
        }
    }
}

impl TryFrom<&PersistedEntry> for EventSnapshot {
    type Error = StateError;

    fn try_from(entry: &PersistedEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            serial: entry.serial,
            is_final: entry.is_final,
            is_cancelled: entry.is_cancelled,
            max_intensity: entry
                .max_intensity
                .as_deref()
                .map(parse_persisted_intensity)
                .transpose()?,
            last_seen_epoch_seconds: entry.last_seen_epoch_seconds,
        })
    }
}

impl PersistedEntry {
    fn from_snapshot(key: &str, snapshot: &EventSnapshot) -> Self {
        Self {
            key: key.to_owned(),
            serial: snapshot.serial,
            is_final: snapshot.is_final,
            is_cancelled: snapshot.is_cancelled,
            max_intensity: snapshot
                .max_intensity
                .map(|intensity| intensity.to_string()),
            last_seen_epoch_seconds: snapshot.last_seen_epoch_seconds,
        }
    }
}

fn parse_persisted_intensity(value: &str) -> Result<Intensity, StateError> {
    value
        .parse()
        .map_err(|source| StateError::InvalidIntensity {
            value: value.to_owned(),
            source,
        })
}

fn current_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn serial_advanced(previous: Option<u32>, current: Option<u32>) -> bool {
    match (previous, current) {
        (Some(previous), Some(current)) => current > previous,
        (None, Some(_)) => true,
        _ => false,
    }
}

fn intensity_increased(previous: Option<Intensity>, current: Option<Intensity>) -> bool {
    match (previous, current) {
        (Some(previous), Some(current)) => current > previous,
        (None, Some(Intensity::Unknown)) | (None, None) | (Some(_), None) => false,
        (None, Some(_)) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventKind, Hypocenter, Magnitude};
    use std::fs;

    fn event(serial: Option<u32>, intensity: Option<Intensity>) -> EarthquakeEvent {
        EarthquakeEvent {
            source_id: Some("event-1".to_owned()),
            kind: EventKind::Earthquake,
            serial,
            is_final: false,
            is_cancelled: false,
            occurred_at: Some("2026-06-26T12:00:00+09:00".to_owned()),
            announced_at: Some("2026-06-26T12:01:00+09:00".to_owned()),
            hypocenter: Some(Hypocenter {
                name: "東京都23区".to_owned(),
                depth_km: Some(40),
            }),
            max_intensity: intensity,
            magnitude: Some(Magnitude(5.2)),
            affected_areas: vec!["東京都".to_owned()],
        }
    }

    #[test]
    fn treats_first_event_as_new() {
        let state = DedupState::new();

        assert_eq!(
            state.check(&event(Some(1), Some(Intensity::Three))),
            DedupDecision::New
        );
    }

    #[test]
    fn treats_same_event_as_duplicate_after_marking() {
        let mut state = DedupState::new();
        let event = event(Some(1), Some(Intensity::Three));

        assert_eq!(state.should_process_and_mark(&event), DedupDecision::New);
        assert_eq!(
            state.should_process_and_mark(&event),
            DedupDecision::Duplicate
        );
    }

    #[test]
    fn treats_serial_advance_as_update() {
        let mut state = DedupState::new();
        state.mark_processed(&event(Some(1), Some(Intensity::Three)));

        assert_eq!(
            state.check(&event(Some(2), Some(Intensity::Three))),
            DedupDecision::Updated(vec![EventChange::SerialAdvanced])
        );
    }

    #[test]
    fn treats_intensity_increase_as_update() {
        let mut state = DedupState::new();
        state.mark_processed(&event(Some(1), Some(Intensity::Three)));

        assert_eq!(
            state.check(&event(Some(1), Some(Intensity::Four))),
            DedupDecision::Updated(vec![EventChange::IntensityIncreased])
        );
    }

    #[test]
    fn treats_cancel_as_update() {
        let mut state = DedupState::new();
        state.mark_processed(&event(Some(1), Some(Intensity::Three)));
        let mut cancelled = event(Some(1), Some(Intensity::Three));
        cancelled.is_cancelled = true;

        assert_eq!(
            state.check(&cancelled),
            DedupDecision::Updated(vec![EventChange::BecameCancelled])
        );
    }

    #[test]
    fn persists_and_loads_processed_events() {
        let path = temp_state_path("persist-load");
        let event = event(Some(1), Some(Intensity::Three));
        let mut state = DedupState::new();
        state.mark_processed(&event);

        state
            .save_to_path(&path, 100)
            .expect("state should save to disk");
        let loaded = DedupState::load_from_path(&path, 100).expect("state should load from disk");

        assert_eq!(loaded.check(&event), DedupDecision::Duplicate);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn prunes_old_entries_when_saving() {
        let path = temp_state_path("prune");
        let mut state = DedupState::new();
        for index in 0..3 {
            let mut event = event(Some(index), Some(Intensity::Three));
            event.source_id = Some(format!("event-{index}"));
            state.mark_processed(&event);
        }

        state
            .save_to_path(&path, 2)
            .expect("state should save to disk");
        let loaded = DedupState::load_from_path(&path, 100).expect("state should load from disk");

        assert_eq!(loaded.len(), 2);

        let _ = fs::remove_file(path);
    }

    fn temp_state_path(name: &str) -> std::path::PathBuf {
        let unique = current_epoch_seconds();
        std::env::temp_dir().join(format!("eew-quake-notify-{name}-{unique}.json"))
    }
}
