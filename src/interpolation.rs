//! Flashback's sided interpolation and time-aware centripetal curves.
//! Algorithms derived from Moulberry's Flashback; original copyright retained.
use crate::timeline::{Interpolation, Key};

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Linear,
    Ease,
    Hold,
    Smooth,
    Hermite,
}
fn sides(mode: Interpolation) -> (Side, Side) {
    match mode {
        Interpolation::Step => (Side::Hold, Side::Hold),
        Interpolation::Linear => (Side::Linear, Side::Linear),
        Interpolation::Smooth => (Side::Smooth, Side::Smooth),
        Interpolation::EaseIn => (Side::Ease, Side::Linear),
        Interpolation::EaseOut => (Side::Linear, Side::Ease),
        Interpolation::EaseInOut => (Side::Ease, Side::Ease),
        Interpolation::Hermite => (Side::Hermite, Side::Hermite),
    }
}
fn special(side: Side) -> bool {
    matches!(side, Side::Smooth | Side::Hermite)
}
fn amount(mut left: Side, mut right: Side, t: f64) -> f64 {
    if special(left) {
        if special(right) {
            left = Side::Linear;
            right = Side::Linear;
        } else {
            left = right;
        }
    } else if special(right) {
        right = left;
    }
    if left == Side::Hold {
        return 0.0;
    }
    if right == Side::Hold {
        right = left;
    }
    match (left, right) {
        (Side::Ease, Side::Linear) => t.powi(3),
        (Side::Linear, Side::Ease) => 1.0 - (1.0 - t).powi(3),
        (Side::Ease, Side::Ease) if t < 0.5 => 4.0 * t.powi(3),
        (Side::Ease, Side::Ease) => 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0,
        _ => t,
    }
}
fn delta(a: f64, b: f64, period: f64) -> f64 {
    if period > 0.0 {
        (b - a + period / 2.0).rem_euclid(period) - period / 2.0
    } else {
        b - a
    }
}
fn blend<const N: usize>(a: [f64; N], b: [f64; N], t: f64, periods: [f64; N]) -> [f64; N] {
    std::array::from_fn(|i| a[i] + delta(a[i], b[i], periods[i]) * t)
}

#[derive(Clone, Copy)]
pub(crate) enum Boundary {
    Hold,
    Original,
}

/// Discrete tracking targets follow the same sided blend as numeric channels.
pub(crate) fn discrete_index<T>(keys: &[Key<T>], tick: f64) -> Option<usize> {
    if keys.is_empty() || !tick.is_finite() || tick < keys[0].tick as f64 {
        return None;
    }
    let lo = keys
        .partition_point(|k| k.tick as f64 <= tick)
        .saturating_sub(1);
    if tick == keys[lo].tick as f64 || keys[lo].interpolation == Interpolation::Step {
        return Some(lo);
    }
    let hi = lo + 1;
    if hi == keys.len() {
        return (tick.floor() == keys[lo].tick as f64).then_some(lo);
    }
    let a = (tick - keys[lo].tick as f64) / (keys[hi].tick - keys[lo].tick) as f64;
    let left = sides(keys[lo].interpolation).1;
    let mut right = sides(keys[hi].interpolation).0;
    if right == Side::Hold {
        right = left;
    }
    let chosen = if a < 0.5 { left } else { right };
    let blend = if special(chosen) {
        a
    } else {
        amount(left, right, a)
    };
    Some(if blend < 0.5 { lo } else { hi })
}

/// Sample components without allocating on linear/ease/smooth render paths.
/// Position groups share Euclidean centripetal knots. Other channels use their
/// scalar distances; nonzero periods unwrap rotation before spline evaluation.
pub(crate) fn sample<T, const N: usize>(
    keys: &[Key<T>],
    tick: f64,
    values: impl Fn(&T) -> [f64; N],
    periods: [f64; N],
    positions: &[[usize; 3]],
    boundary: Boundary,
) -> Option<[f64; N]> {
    if keys.is_empty() || !tick.is_finite() {
        return None;
    }
    if tick < keys[0].tick as f64 {
        return matches!(boundary, Boundary::Hold).then(|| values(&keys[0].value));
    }
    let lo = keys
        .partition_point(|k| k.tick as f64 <= tick)
        .saturating_sub(1);
    let low = &keys[lo];
    if tick == low.tick as f64 || low.interpolation == Interpolation::Step {
        return Some(values(&low.value));
    }
    if lo + 1 == keys.len() {
        return (matches!(boundary, Boundary::Hold) || tick.floor() == low.tick as f64)
            .then(|| values(&low.value));
    }
    let hi = lo + 1;
    let a = (tick - low.tick as f64) / (keys[hi].tick - low.tick) as f64;
    let left = sides(low.interpolation).1;
    let mut right = sides(keys[hi].interpolation).0;
    if right == Side::Hold {
        right = left;
    }
    let linear = blend(
        values(&low.value),
        values(&keys[hi].value),
        amount(left, right, a),
        periods,
    );
    let smooth = if left == Side::Smooth || right == Side::Smooth {
        let before = if lo > 0 && keys[lo - 1].interpolation != Interpolation::Step {
            lo - 1
        } else {
            lo
        };
        let after = if hi + 1 < keys.len() && keys[hi].interpolation != Interpolation::Step {
            hi + 1
        } else {
            hi
        };
        let indices = [before, lo, hi, after];
        let points = indices.map(|i| values(&keys[i].value));
        let times = indices.map(|i| (keys[i].tick - keys[before].tick) as f64);
        Some(smooth_values(points, times, a, periods, positions))
    } else {
        None
    };
    let polynomial = if left == Side::Hermite || right == Side::Hermite {
        let mut start = lo;
        while start > 0 && keys[start - 1].interpolation != Interpolation::Step {
            start -= 1;
        }
        let mut end = hi;
        while end + 1 < keys.len() {
            end += 1;
            if keys[end].interpolation == Interpolation::Step {
                break;
            }
        }
        Some(polynomial(&keys[start..=end], tick, &values, periods))
    } else {
        None
    };
    let evaluate = |side| match side {
        Side::Smooth => smooth.unwrap_or(linear),
        Side::Hermite => polynomial.unwrap_or(linear),
        _ => linear,
    };
    let result = blend(evaluate(left), evaluate(right), a, periods);
    result.iter().all(|x| x.is_finite()).then_some(result)
}

fn smooth_values<const N: usize>(
    mut points: [[f64; N]; 4],
    times: [f64; 4],
    amount: f64,
    periods: [f64; N],
    groups: &[[usize; 3]],
) -> [f64; N] {
    for (c, period) in periods.iter().copied().enumerate() {
        if period > 0.0 {
            points[0][c] = (points[0][c] + period / 2.0).rem_euclid(period) - period / 2.0;
            for p in 1..4 {
                points[p][c] = points[p - 1][c] + delta(points[p - 1][c], points[p][c], period);
            }
        }
    }
    std::array::from_fn(|c| {
        let distances = if let Some(group) = groups.iter().find(|group| group.contains(&c)) {
            std::array::from_fn(|p| {
                let d = group.map(|i| points[p + 1][i] - points[p][i]);
                d[0].hypot(d[1]).hypot(d[2]).sqrt()
            })
        } else {
            std::array::from_fn(|p| (points[p + 1][c] - points[p][c]).abs().sqrt())
        };
        centripetal(points.map(|p| p[c]), times, distances, amount)
    })
}
fn centripetal(points: [f64; 4], times: [f64; 4], distances: [f64; 3], amount: f64) -> f64 {
    let average = distances.iter().sum::<f64>() / 3.0;
    if average == 0.0 {
        return points[1];
    }
    let relation = times[3] / 3.0 / average;
    let mut knots = [0.0; 4];
    for i in 0..3 {
        let elapsed = times[i + 1] - times[i];
        let duration = if elapsed > 0.0 {
            let scaled = distances[i] * relation;
            scaled / (scaled / elapsed).clamp(0.4, 2.5)
        } else {
            0.0
        };
        knots[i + 1] = knots[i] + duration;
    }
    let t = knots[1] + (knots[2] - knots[1]) * amount;
    let lerp = |a: f64, b: f64, l: usize, h: usize| {
        let fraction = if knots[l] == knots[h] {
            0.5
        } else {
            (t - knots[l]) / (knots[h] - knots[l])
        };
        a + (b - a) * fraction
    };
    let a = [
        lerp(points[0], points[1], 0, 1),
        lerp(points[1], points[2], 1, 2),
        lerp(points[2], points[3], 2, 3),
    ];
    lerp(lerp(a[0], a[1], 0, 2), lerp(a[1], a[2], 1, 3), 1, 2)
}

// With no derivative samples, Apache Commons' HermiteInterpolator evaluates
// the unique interpolation polynomial. Newton divided differences do the same.
fn polynomial<T, const N: usize>(
    keys: &[Key<T>],
    tick: f64,
    values: &impl Fn(&T) -> [f64; N],
    periods: [f64; N],
) -> [f64; N] {
    let origin = keys[0].tick;
    let mut coefficients: Vec<_> = keys.iter().map(|k| values(&k.value)).collect();
    for c in 0..N {
        if periods[c] > 0.0 {
            coefficients[0][c] =
                (coefficients[0][c] + periods[c] / 2.0).rem_euclid(periods[c]) - periods[c] / 2.0;
            for i in 1..coefficients.len() {
                coefficients[i][c] = coefficients[i - 1][c]
                    + delta(coefficients[i - 1][c], coefficients[i][c], periods[c]);
            }
        }
    }
    for order in 1..keys.len() {
        for i in (order..keys.len()).rev() {
            let denominator = (keys[i].tick - keys[i - order].tick) as f64;
            let previous = coefficients[i - 1];
            for (value, previous) in coefficients[i].iter_mut().zip(previous) {
                *value = (*value - previous) / denominator;
            }
        }
    }
    let t = tick - origin as f64;
    let mut result = *coefficients.last().expect("nonempty interpolation span");
    for i in (0..coefficients.len() - 1).rev() {
        for c in 0..N {
            result[c] = result[c] * (t - (keys[i].tick - origin) as f64) + coefficients[i][c];
        }
    }
    for (value, period) in result.iter_mut().zip(periods) {
        if period > 0.0 {
            *value = (*value + period / 2.0).rem_euclid(period) - period / 2.0;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cinematic::{TrackData, TrackSample};
    use crate::timeline::{CameraKey, Vec3};

    fn key<T>(tick: u64, value: T, interpolation: Interpolation) -> Key<T> {
        Key {
            tick,
            value,
            interpolation,
        }
    }
    fn close(a: f64, b: f64, tolerance: f64) {
        assert!((a - b).abs() <= tolerance, "{a} != {b}");
    }

    #[test]
    fn curves_match_pinned_upstream_java_outputs() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/interpolation-java.json"))
                .unwrap();
        let scalar = TrackData::Speed(
            [0, 3, 20, 35]
                .into_iter()
                .zip([10.0, 20.0, 100.0, 0.0])
                .map(|(t, v)| key(t, v, Interpolation::Smooth))
                .collect(),
        );
        let positions = [
            Vec3::default(),
            Vec3 {
                x: 2.0,
                y: 4.0,
                z: -1.0,
            },
            Vec3 {
                x: 12.0,
                y: 3.0,
                z: 8.0,
            },
            Vec3 {
                x: 13.0,
                y: 20.0,
                z: 8.0,
            },
        ];
        let camera = TrackData::Camera(
            [0, 3, 20, 35]
                .into_iter()
                .zip(positions)
                .zip([350.0, 5.0, 35.0, 80.0])
                .map(|((t, position), yaw)| {
                    key(
                        t,
                        CameraKey {
                            position,
                            yaw,
                            pitch: 0.0,
                            roll: 0.0,
                        },
                        Interpolation::Smooth,
                    )
                })
                .collect(),
        );
        for row in fixture["smooth"].as_array().unwrap() {
            let tick = 3.0 + 17.0 * row["amount"].as_f64().unwrap();
            let TrackSample::Speed(value) = scalar.sample(tick, false).unwrap() else {
                panic!()
            };
            close(value, row["scalar"].as_f64().unwrap(), 2e-4);
            let TrackSample::Camera(value) = camera.sample(tick, false).unwrap() else {
                panic!()
            };
            close(value.yaw, row["angle"].as_f64().unwrap(), 2e-4);
            for (i, v) in [value.position.x, value.position.y, value.position.z]
                .into_iter()
                .enumerate()
            {
                close(v, row["position"][i].as_f64().unwrap(), 2e-6);
            }
        }
        let side = |s: &str| match s {
            "LINEAR" => Side::Linear,
            "EASE" => Side::Ease,
            "HOLD" => Side::Hold,
            "SMOOTH" => Side::Smooth,
            "HERMITE" => Side::Hermite,
            _ => panic!(),
        };
        for row in fixture["easing"].as_array().unwrap() {
            for (i, t) in [0.0, 0.125, 0.25, 0.5, 0.75, 0.875, 1.0]
                .into_iter()
                .enumerate()
            {
                close(
                    amount(
                        side(row["left"].as_str().unwrap()),
                        side(row["right"].as_str().unwrap()),
                        t,
                    ),
                    row["values"][i].as_f64().unwrap(),
                    1e-7,
                );
            }
        }
    }

    #[test]
    fn hermite_uses_full_span_and_stops_at_hold_boundaries() {
        let keys = vec![
            key(0, 0.0, Interpolation::Hermite),
            key(10, 100.0, Interpolation::Hermite),
            key(20, 400.0, Interpolation::Hermite),
        ];
        close(
            sample(&keys, 5.0, |v| [*v], [0.0], &[], Boundary::Original).unwrap()[0],
            25.0,
            1e-10,
        );
        close(
            sample(&keys, 15.0, |v| [*v], [0.0], &[], Boundary::Original).unwrap()[0],
            225.0,
            1e-10,
        );
        let mut keys = keys;
        keys.insert(0, key(0, 1e10, Interpolation::Step));
        keys[1].tick = 1;
        // The hold key to the left must not pull the polynomial toward its value.
        let value = sample(&keys, 5.0, |v| [*v], [0.0], &[], Boundary::Original).unwrap()[0];
        assert!(value.abs() < 100.0);
        assert!(sample(&keys, -1.0, |v| [*v], [0.0], &[], Boundary::Original).is_none());
        assert!(sample(&keys, 21.0, |v| [*v], [0.0], &[], Boundary::Original).is_none());
    }
}
