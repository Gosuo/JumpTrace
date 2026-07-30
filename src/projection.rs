#[derive(Clone, Copy, Debug)]
pub struct ScreenCalibration {
    pub north: [f64; 2],
    pub right_axis: [f64; 2],
}

impl ScreenCalibration {
    pub fn minimum_angular_error_deg(
        self,
        universe_vector: [f64; 3],
        tunnel: [f64; 2],
    ) -> Option<f64> {
        let projected_y = self.projected_y_axes()?;
        let tunnel_side = cross(self.north, tunnel);
        let mut minimum_error = f64::INFINITY;

        for y_axis in projected_y {
            // The marked screen-right axis corresponds to local -X. Since local X is
            // the inverse of universe X, universe +X maps directly to this axis.
            let projected = add(
                add(
                    scale(self.right_axis, universe_vector[0]),
                    scale(y_axis, universe_vector[1]),
                ),
                scale(self.north, universe_vector[2]),
            );
            let projected_side = cross(self.north, projected);
            if tunnel_side * projected_side < 0.0 {
                continue;
            }
            if let Some(error) = vector_angle_deg(projected, tunnel) {
                minimum_error = minimum_error.min(error);
            }
        }

        minimum_error.is_finite().then_some(minimum_error)
    }

    fn projected_y_axes(self) -> Option<[[f64; 2]; 2]> {
        let x = self.right_axis;
        let z = self.north;
        let a = dot(x, x);
        let b = dot(x, z);
        let c = dot(z, z);
        let scale_squared = ((a + c) + ((a - c).powi(2) + 4.0 * b.powi(2)).sqrt()) / 2.0;
        if scale_squared <= f64::EPSILON {
            return None;
        }

        let screen_scale = scale_squared.sqrt();
        let row_x = [x[0] / screen_scale, z[0] / screen_scale];
        let row_y = [x[1] / screen_scale, z[1] / screen_scale];
        let missing_x = (1.0 - dot(row_x, row_x)).max(0.0).sqrt();
        let missing_y_magnitude = (1.0 - dot(row_y, row_y)).max(0.0).sqrt();
        let required_product = -dot(row_x, row_y);
        let missing_y = if missing_x > 1e-9 {
            required_product / missing_x
        } else {
            missing_y_magnitude
        };
        let projected_y = [missing_x * screen_scale, missing_y * screen_scale];

        Some([projected_y, scale(projected_y, -1.0)])
    }
}

fn vector_angle_deg(left: [f64; 2], right: [f64; 2]) -> Option<f64> {
    let lengths = dot(left, left).sqrt() * dot(right, right).sqrt();
    if lengths <= f64::EPSILON {
        return None;
    }

    Some(
        (dot(left, right) / lengths)
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees(),
    )
}

fn add(left: [f64; 2], right: [f64; 2]) -> [f64; 2] {
    [left[0] + right[0], left[1] + right[1]]
}

fn scale(vector: [f64; 2], factor: f64) -> [f64; 2] {
    [vector[0] * factor, vector[1] * factor]
}

fn dot(left: [f64; 2], right: [f64; 2]) -> f64 {
    left[0] * right[0] + left[1] * right[1]
}

fn cross(left: [f64; 2], right: [f64; 2]) -> f64 {
    left[0] * right[1] - left[1] * right[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_on_calibration_projects_planar_vectors() {
        let calibration = ScreenCalibration {
            north: [0.0, -1.0],
            right_axis: [1.0, 0.0],
        };
        let error = calibration
            .minimum_angular_error_deg([1.0, 0.0, 1.0], [1.0, -1.0])
            .expect("projection should be valid");

        assert!(error < 1e-5);
    }

    #[test]
    fn candidate_on_opposite_side_of_north_is_rejected() {
        let calibration = ScreenCalibration {
            north: [0.0, -1.0],
            right_axis: [1.0, 0.0],
        };

        assert!(
            calibration
                .minimum_angular_error_deg([-1.0, 0.0, 1.0], [1.0, -1.0])
                .is_none()
        );
    }

    #[test]
    fn invalid_zero_length_calibration_is_rejected() {
        let calibration = ScreenCalibration {
            north: [0.0, 0.0],
            right_axis: [0.0, 0.0],
        };

        assert!(
            calibration
                .minimum_angular_error_deg([1.0, 1.0, 1.0], [1.0, 0.0])
                .is_none()
        );
    }
}
