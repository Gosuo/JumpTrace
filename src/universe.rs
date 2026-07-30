use std::collections::HashSet;

use serde::Deserialize;

const SOLAR_SYSTEMS_CSV: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/mapSolarSystems.csv"
));

#[derive(Debug)]
pub struct SolarSystem {
    pub region_id: u32,
    pub constellation_id: u32,
    pub id: u32,
    pub name: String,
    pub position: [f64; 3],
    pub security: f64,
    pub security_class: Option<String>,
}

#[derive(Debug)]
pub struct Universe {
    pub systems: Vec<SolarSystem>,
}

impl Universe {
    pub fn load_embedded() -> Result<Self, String> {
        Self::from_csv(SOLAR_SYSTEMS_CSV)
    }

    fn from_csv(data: &str) -> Result<Self, String> {
        let mut reader = csv::Reader::from_reader(data.as_bytes());
        let mut systems = Vec::new();
        let mut ids = HashSet::new();

        for row in reader.deserialize::<SolarSystemRow>() {
            let row = row.map_err(|error| format!("Invalid solar-system CSV row: {error}"))?;

            if !ids.insert(row.solar_system_id) {
                return Err(format!(
                    "Duplicate solar-system ID {} in embedded data",
                    row.solar_system_id
                ));
            }
            if row.solar_system_name.trim().is_empty() {
                return Err(format!(
                    "Solar-system ID {} has an empty name",
                    row.solar_system_id
                ));
            }
            if !row.x.is_finite()
                || !row.y.is_finite()
                || !row.z.is_finite()
                || !row.security.is_finite()
            {
                return Err(format!(
                    "Solar system {} contains a non-finite number",
                    row.solar_system_name
                ));
            }

            systems.push(SolarSystem {
                region_id: row.region_id,
                constellation_id: row.constellation_id,
                id: row.solar_system_id,
                name: row.solar_system_name,
                position: [row.x, row.y, row.z],
                security: row.security,
                security_class: row.security_class,
            });
        }

        if systems.is_empty() {
            return Err("Embedded solar-system data is empty".to_owned());
        }

        Ok(Self { systems })
    }
}

#[derive(Deserialize)]
struct SolarSystemRow {
    #[serde(rename = "regionID")]
    region_id: u32,
    #[serde(rename = "constellationID")]
    constellation_id: u32,
    #[serde(rename = "solarSystemID")]
    solar_system_id: u32,
    #[serde(rename = "solarSystemName")]
    solar_system_name: String,
    x: f64,
    y: f64,
    z: f64,
    security: f64,
    #[serde(rename = "securityClass")]
    security_class: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_universe_loads() {
        let universe = Universe::load_embedded().expect("embedded SDE data should be valid");

        assert!(universe.systems.len() > 7_000);
        let tanoo = universe
            .systems
            .iter()
            .find(|system| system.id == 30_000_001)
            .expect("Tanoo should be present");
        assert_eq!(tanoo.name, "Tanoo");
        assert_eq!(tanoo.region_id, 10_000_001);
        assert_eq!(tanoo.constellation_id, 20_000_001);
        assert!(
            tanoo
                .position
                .iter()
                .all(|coordinate| coordinate.is_finite())
        );
        assert!(tanoo.security.is_finite());
        assert_eq!(tanoo.security_class.as_deref(), Some("B"));
    }
}
