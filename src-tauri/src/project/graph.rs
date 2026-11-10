//! Where the pages of the graph view go, in 3D: computed here, away from the
//! interface, which would freeze while placing a big project.

/// A point in space.
type P3 = [f64; 3];

/// Distance the links pull the pages to.
const EDGE: f64 = 60.0;
/// Past this distance, pages no longer push each other.
const REACH: f64 = 4.0 * EDGE;

/// Where each of the `n` pages goes: they push each other away and the links
/// pull them together (Fruchterman-Reingold), started on a spiral so the same
/// project is always drawn the same way.
pub fn layout(n: usize, links: &[(usize, usize)]) -> Vec<P3> {
    let mut pos: Vec<P3> = (0..n).map(|i| {
        // A spiral wound around a ball growing with the pages
        let f = i as f64;
        let (r, a, h) = (EDGE * f.cbrt(), f * 2.399_963, 1.0 - 2.0 * ((f * 0.618_034) % 1.0));
        let flat = (1.0 - h * h).sqrt();
        [r * flat * a.cos(), r * h, r * flat * a.sin()]
    }).collect();
    let mut temp = EDGE * (n as f64).cbrt().max(1.0) * 2.0;
    for _ in 0..300 {
        let shift = &mut repulse(&pos);
        for &(a, b) in links {
            let d: P3 = std::array::from_fn(|k| pos[a][k] - pos[b][k]);
            let f = len(&d) / EDGE;
            for k in 0..3 {
                shift[a][k] -= d[k] * f;
                shift[b][k] += d[k] * f;
            }
        }
        let mut most = 0.0_f64;
        for (p, s) in pos.iter_mut().zip(shift.iter()) {
            // A pull to the middle, so pages without links don't drift away
            let s: P3 = std::array::from_fn(|k| s[k] - p[k] * 0.05);
            let l = len(&s).max(0.01);
            let step = l.min(temp) / l;
            for k in 0..3 { p[k] += s[k] * step; }
            most = most.max(l.min(temp));
        }
        // Settled: no page moves by more than a pixel or so
        if most < 0.5 { break; }
        temp = (temp * 0.97).max(0.5);
    }
    pos
}

fn len(v: &P3) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// How much the pages push each other away. Only near pages push: from afar,
/// a whole project would throw the lone pages out of sight. So each page is
/// only compared with those of its cube and the 26 around, cubes as wide as
/// that reach, instead of with every other page.
fn repulse(pos: &[P3]) -> Vec<P3> {
    let cell = |p: &P3| p.map(|c| (c / REACH).floor() as i64);
    let mut grid: std::collections::HashMap<[i64; 3], Vec<usize>> = Default::default();
    for (i, p) in pos.iter().enumerate() { grid.entry(cell(p)).or_default().push(i); }
    let mut shift = vec![[0.0; 3]; pos.len()];
    for (i, p) in pos.iter().enumerate() {
        let [cx, cy, cz] = cell(p);
        for gx in cx - 1..=cx + 1 {
            for gy in cy - 1..=cy + 1 {
                for gz in cz - 1..=cz + 1 {
                    for &j in grid.get(&[gx, gy, gz]).into_iter().flatten() {
                        if j == i { continue; }
                        let d: P3 = std::array::from_fn(|k| p[k] - pos[j][k]);
                        let d2 = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).max(0.01);
                        if d2 > REACH * REACH { continue; }
                        let f = EDGE * EDGE / d2;
                        for k in 0..3 { shift[i][k] += d[k] * f; }
                    }
                }
            }
        }
    }
    shift
}

#[cfg(test)]
mod tests {
    use super::{layout, len, P3};

    fn dist(a: &P3, b: &P3) -> f64 {
        len(&std::array::from_fn(|k| a[k] - b[k]))
    }

    /// The cubes give what comparing every pair of pages gives.
    #[test]
    fn repulse_by_cubes_is_every_pair() {
        let pos: Vec<P3> = (0..300).map(|i| {
            let i = i as f64;
            [(i * 37.0) % 1500.0 - 750.0, (i * 91.0) % 1300.0 - 650.0, (i * 53.0) % 900.0 - 450.0]
        }).collect();
        let mut all = vec![[0.0; 3]; pos.len()];
        for i in 0..pos.len() {
            for j in 0..pos.len() {
                let d2 = dist(&pos[i], &pos[j]).powi(2).max(0.01);
                if i == j || d2 > super::REACH * super::REACH { continue; }
                let f = super::EDGE * super::EDGE / d2;
                for k in 0..3 { all[i][k] += (pos[i][k] - pos[j][k]) * f; }
            }
        }
        for (a, b) in super::repulse(&pos).iter().zip(&all) {
            assert!(dist(a, b) < 1e-6, "{a:?} {b:?}");
        }
    }

    #[test]
    fn lone_pages_stay_near_the_others() {
        // 200 pages, a linked core of 20 and 180 pages without links
        let links: Vec<(usize, usize)> = (1..20).map(|i| (0, i)).collect();
        let pos = layout(200, &links);
        let all = pos.iter().map(|p| len(p)).fold(0.0, f64::max);
        assert!(all < 25.0 * super::EDGE, "{all}");
    }

    #[test]
    fn linked_pages_end_closer_than_the_others() {
        let pos = layout(4, &[(0, 1), (1, 2)]);
        let d = |a: usize, b: usize| dist(&pos[a], &pos[b]);
        assert!(pos.iter().all(|p| p.iter().all(|c| c.is_finite())));
        assert!(d(0, 1) < d(0, 3) && d(1, 2) < d(2, 3), "{pos:?}");
        assert!(layout(0, &[]).is_empty());
        assert_eq!(layout(1, &[]).len(), 1);
    }
}
