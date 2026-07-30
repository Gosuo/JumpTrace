use std::collections::HashSet;

use serde::Deserialize;

const SOLAR_SYSTEMS_CSV: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/mapSolarSystems.csv"
));
const METERS_PER_LIGHT_YEAR: f64 = 9_460_730_472_580_800.0;
const POCHVEN_REGION_ID: u32 = 10_000_070;
const KNOWN_SPACE_SYSTEM_ID_START: u32 = 30_000_000;
const KNOWN_SPACE_SYSTEM_ID_END: u32 = 31_000_000;
const HIGH_SECURITY_THRESHOLD: f64 = 0.45;

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

#[derive(Debug)]
pub struct ReachableSystem<'a> {
    pub system: &'a SolarSystem,
    pub distance_ly: f64,
    pub bearing_deg: f64,
    pub angular_error_deg: f64,
}

impl Universe {
    pub fn load_embedded() -> Result<Self, String> {
        Self::from_csv(SOLAR_SYSTEMS_CSV)
    }

    pub fn system(&self, id: u32) -> Option<&SolarSystem> {
        self.systems.iter().find(|system| system.id == id)
    }

    pub fn systems_matching_jump_bearing(
        &self,
        origin: &SolarSystem,
        maximum_range_ly: f64,
        measured_bearing_deg: f64,
    ) -> Vec<ReachableSystem<'_>> {
        let mut reachable: Vec<_> = self
            .systems
            .iter()
            .filter(|system| system.id != origin.id && system.is_static_jump_destination())
            .filter_map(|system| {
                let distance_ly = distance_ly(origin.position, system.position);
                if distance_ly > maximum_range_ly {
                    return None;
                }

                let bearing_deg = map_bearing_deg(origin.position, system.position)?;
                Some(ReachableSystem {
                    system,
                    distance_ly,
                    bearing_deg,
                    angular_error_deg: angular_difference_deg(measured_bearing_deg, bearing_deg),
                })
            })
            .collect();

        reachable.sort_by(|left, right| {
            left.angular_error_deg
                .total_cmp(&right.angular_error_deg)
                .then_with(|| left.distance_ly.total_cmp(&right.distance_ly))
        });
        reachable
    }

    pub fn search_systems(&self, query: &str, limit: usize) -> Vec<&SolarSystem> {
        let query = query.trim().to_lowercase();
        if query.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut matches: Vec<_> = self
            .systems
            .iter()
            .filter(|system| system.name.to_lowercase().starts_with(&query))
            .take(limit)
            .collect();

        if matches.len() < limit {
            matches.extend(
                self.systems
                    .iter()
                    .filter(|system| {
                        let name = system.name.to_lowercase();
                        !name.starts_with(&query) && name.contains(&query)
                    })
                    .take(limit - matches.len()),
            );
        }

        matches
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

impl SolarSystem {
    fn is_static_jump_destination(&self) -> bool {
        (KNOWN_SPACE_SYSTEM_ID_START..KNOWN_SPACE_SYSTEM_ID_END).contains(&self.id)
            && self.region_id != POCHVEN_REGION_ID
            && self.security < HIGH_SECURITY_THRESHOLD
    }
}

fn map_bearing_deg(origin: [f64; 3], destination: [f64; 3]) -> Option<f64> {
    let east = destination[0] - origin[0];
    let north = destination[2] - origin[2];
    (east != 0.0 || north != 0.0).then(|| east.atan2(north).to_degrees().rem_euclid(360.0))
}

fn angular_difference_deg(left: f64, right: f64) -> f64 {
    ((left - right + 180.0).rem_euclid(360.0) - 180.0).abs()
}

fn distance_ly(left: [f64; 3], right: [f64; 3]) -> f64 {
    let squared_distance: f64 = left
        .into_iter()
        .zip(right)
        .map(|(left, right)| (left - right).powi(2))
        .sum();
    squared_distance.sqrt() / METERS_PER_LIGHT_YEAR
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

    #[test]
    fn search_prioritizes_name_prefixes() {
        let universe = Universe::load_embedded().expect("embedded SDE data should be valid");
        let matches = universe.search_systems("tan", 5);

        assert!(!matches.is_empty());
        assert_eq!(matches[0].name, "Tanoo");
        assert!(
            matches
                .iter()
                .all(|system| system.name.to_lowercase().contains("tan"))
        );
    }

    #[test]
    fn jump_candidates_are_filtered_and_ranked_by_bearing() {
        let one_light_year = METERS_PER_LIGHT_YEAR;
        let csv = format!(
            "regionID,constellationID,solarSystemID,solarSystemName,x,y,z,security,securityClass\n\
             10000001,20000001,30000001,Origin,0,0,0,-0.1,A\n\
             10000001,20000001,30000002,Far,{},0,{},-0.1,A\n\
             10000001,20000001,30000003,Near,{},0,0,-0.1,A\n\
             10000001,20000001,30000004,Highsec,{},0,0,0.9,A\n\
             10000070,20000001,30000005,Pochven,{},0,0,-1.0,A\n\
             11000001,21000001,31000001,Wormhole,{},0,0,-1.0,A",
            one_light_year,
            one_light_year,
            one_light_year * 0.5,
            one_light_year * 0.25,
            one_light_year * 0.25,
            one_light_year * 0.25,
        );
        let universe = Universe::from_csv(&csv).expect("fixture should load");
        let origin = universe.system(30_000_001).expect("origin should exist");
        let reachable = universe.systems_matching_jump_bearing(origin, 2.0, 80.0);

        let names: Vec<_> = reachable
            .iter()
            .map(|candidate| candidate.system.name.as_str())
            .collect();
        assert_eq!(names, ["Near", "Far"]);
        assert!((reachable[0].bearing_deg - 90.0).abs() < 1e-12);
        assert!((reachable[0].angular_error_deg - 10.0).abs() < 1e-12);
        assert!((reachable[0].distance_ly - 0.5).abs() < 1e-12);
        assert!((reachable[1].bearing_deg - 45.0).abs() < 1e-12);
        assert!((reachable[1].distance_ly - 2.0_f64.sqrt()).abs() < 1e-12);
    }
}
