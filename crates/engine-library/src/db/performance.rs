use super::{Database, columns, records::upsert};
use crate::{Error, Result, performance::PerformanceData};
use rusqlite::{OptionalExtension, ToSql};

impl Database {
    pub fn performance(&self, track_id: i64) -> Result<Option<PerformanceData>> {
        let available = columns(&self.connection, "PerformanceData")?;
        if available.is_empty() {
            return Ok(None);
        }
        let names = [
            "trackId",
            "trackData",
            "overviewWaveFormData",
            "beatData",
            "quickCues",
            "loops",
            "thirdPartySourceId",
            "activeOnLoadLoops",
        ];
        let projection = names
            .iter()
            .map(|name| {
                if available.contains(*name) {
                    (*name).into()
                } else {
                    format!("NULL AS {name}")
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        Ok(self
            .connection
            .query_row(
                &format!("SELECT {projection} FROM PerformanceData WHERE trackId=?"),
                [track_id],
                |row| {
                    Ok(PerformanceData {
                        track_id: row.get(0)?,
                        track_data: row.get(1)?,
                        overview_wave_form_data: row.get(2)?,
                        beat_data: row.get(3)?,
                        quick_cues: row.get(4)?,
                        loops: row.get(5)?,
                        third_party_source_id: row.get(6)?,
                        active_on_load_loops: row.get(7)?,
                    })
                },
            )
            .optional()?)
    }
    pub fn put_performance(&self, data: &PerformanceData) -> Result<()> {
        if self.track(data.track_id)?.is_none() {
            return Err(Error::NotFound(format!("track {}", data.track_id)));
        }
        let available = columns(&self.connection, "PerformanceData")?;
        let pairs: Vec<(&str, &dyn ToSql)> = vec![
            ("trackId", &data.track_id),
            ("trackData", &data.track_data),
            ("overviewWaveFormData", &data.overview_wave_form_data),
            ("beatData", &data.beat_data),
            ("quickCues", &data.quick_cues),
            ("loops", &data.loops),
            ("thirdPartySourceId", &data.third_party_source_id),
            ("activeOnLoadLoops", &data.active_on_load_loops),
        ];
        let pairs: Vec<_> = pairs
            .into_iter()
            .filter(|(name, _)| available.contains(*name))
            .collect();
        if pairs.len() < 2 {
            return Err(Error::Schema("PerformanceData columns are absent".into()));
        }
        upsert(&self.connection, "PerformanceData", "trackId", &pairs)
    }
}
