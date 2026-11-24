//! Cell geometry derived from the effective configuration.

use serde_json::Value;

use super::model;

/// Discretization of the slab: widths, cell centres and region edges.
#[derive(Debug, Clone)]
pub struct Geometry {
    /// Width of every cell \[cm\].
    pub dx: Vec<f64>,
    /// x coordinate of every cell centre \[cm\].
    pub centers: Vec<f64>,
    /// Cumulative region edges (length = regions + 1).
    pub edges: Vec<f64>,
    /// Material zone of each region (1-based).
    pub region_zones: Vec<usize>,
    /// Optional presentation names per zone.
    pub zone_names: Option<Vec<String>>,
}

impl Geometry {
    /// Build the geometry from `NC`, `HR`, `IZL` and `ZONE_NAMES`.
    ///
    /// Returns `None` when the configuration is missing, inconsistent or has
    /// non-positive sizes.
    pub fn from_config(configuration: &Value) -> Option<Geometry> {
        let nc = model::f64_list(configuration, "NC");
        let hr = model::f64_list(configuration, "HR");
        let izl = model::f64_list(configuration, "IZL");
        if nc.is_empty() || nc.len() != hr.len() || nc.len() != izl.len() {
            return None;
        }

        let mut dx = Vec::new();
        let mut centers = Vec::new();
        let mut edges = vec![0.0];
        let mut region_zones = Vec::with_capacity(nc.len());
        let mut x0 = 0.0;

        for ((count, width), zone) in nc.iter().zip(&hr).zip(&izl) {
            if !count.is_finite() || *count < 1.0 || count.fract() != 0.0 {
                return None;
            }
            if !width.is_finite() || *width <= 0.0 {
                return None;
            }
            let cells = *count as usize;
            let cell_width = width / cells as f64;
            for cell in 0..cells {
                dx.push(cell_width);
                centers.push(x0 + (cell as f64 + 0.5) * cell_width);
            }
            x0 += width;
            edges.push(x0);
            region_zones.push(*zone as usize);
        }

        let zone_names = configuration
            .get("ZONE_NAMES")
            .and_then(Value::as_array)
            .map(|names| {
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|names| !names.is_empty());

        Some(Geometry {
            dx,
            centers,
            edges,
            region_zones,
            zone_names,
        })
    }

    /// Label for the zone of a region, using `ZONE_NAMES` when available.
    pub fn zone_label(&self, region: usize) -> String {
        let zone = self.region_zones.get(region).copied().unwrap_or(0);
        self.zone_names
            .as_ref()
            .and_then(|names| names.get(zone.saturating_sub(1)))
            .cloned()
            .unwrap_or_else(|| format!("zone {zone}"))
    }

    /// Number of cells.
    pub fn cells(&self) -> usize {
        self.dx.len()
    }
}

/// Cell centres when the geometry is available, otherwise cell indices.
pub fn centers_or_indices(configuration: Option<&Value>, count: usize) -> Vec<f64> {
    if let Some(configuration) = configuration
        && let Some(geometry) = Geometry::from_config(configuration)
        && geometry.cells() == count
    {
        return geometry.centers;
    }
    (0..count).map(|cell| cell as f64 + 0.5).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn geometry_follows_the_mesh() {
        let config = json!({
            "NC": [2, 3],
            "HR": [2.0, 3.0],
            "IZL": [1, 2],
            "ZONE_NAMES": ["core", "reflector"]
        });
        let geometry = Geometry::from_config(&config).expect("geometry");
        assert_eq!(geometry.cells(), 5);
        assert_eq!(geometry.centers, vec![0.5, 1.5, 2.5, 3.5, 4.5]);
        assert!((geometry.edges[1] - 2.0).abs() < 1e-15);
        assert!((geometry.edges[2] - 5.0).abs() < 1e-15);
        assert_eq!(geometry.zone_label(0), "core");
        assert_eq!(geometry.zone_label(1), "reflector");
    }

    #[test]
    fn invalid_geometry_is_rejected() {
        assert!(Geometry::from_config(&json!({"NC": [0], "HR": [1.0], "IZL": [1]})).is_none());
        assert!(Geometry::from_config(&json!({"NC": [2], "HR": [1.0]})).is_none());
        assert!(Geometry::from_config(&json!({})).is_none());
    }

    #[test]
    fn fallback_uses_cell_indices() {
        assert_eq!(centers_or_indices(None, 3), vec![0.5, 1.5, 2.5]);
        assert_eq!(
            centers_or_indices(Some(&json!({"NC": [3], "HR": [3.0], "IZL": [1]})), 3),
            vec![0.5, 1.5, 2.5]
        );
    }
}
