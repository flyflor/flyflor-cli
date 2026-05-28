use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct ConfirmRecord {
    pub id: String,
    pub status: String,
    pub summary: String,
    pub ask_event_id: Option<String>,
    pub snapshot_id: Option<String>,
    pub source_key: Option<String>,
    pub source_surface: Option<String>,
    pub answer: Value,
    pub raw: Value,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfirmState {
    pub records: Vec<ConfirmRecord>,
}

impl ConfirmState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_records(&mut self, records: Vec<ConfirmRecord>) {
        for record in records {
            self.upsert(record);
        }
    }

    pub fn upsert(&mut self, record: ConfirmRecord) {
        if let Some(existing) = self
            .records
            .iter_mut()
            .find(|existing| existing.id == record.id)
        {
            *existing = record;
        } else {
            self.records.push(record);
        }
    }
}
