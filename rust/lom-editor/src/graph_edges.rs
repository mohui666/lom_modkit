use eframe::egui::{Color32, Painter, Pos2, Shape, Stroke, Vec2};

fn rounded_path(points: &[Pos2], radius: f32) -> Vec<Pos2> {
    let mut corners = Vec::new();
    for &point in points {
        if corners.last() != Some(&point) {
            corners.push(point);
        }
    }
    let Some(&start) = corners.first() else {
        return Vec::new();
    };
    let mut path = vec![start];
    for turn in corners.windows(3) {
        let incoming = turn[1] - turn[0];
        let outgoing = turn[2] - turn[1];
        if incoming.normalized().dot(outgoing.normalized()) > 0.9999 {
            continue;
        }
        let distance = radius
            .min(incoming.length() / 2.0)
            .min(outgoing.length() / 2.0);
        let entry = turn[1] - incoming.normalized() * distance;
        let exit = turn[1] + outgoing.normalized() * distance;
        path.push(entry);
        for step in 1..=8 {
            let t = step as f32 / 8.0;
            path.push(entry.lerp(turn[1], t).lerp(turn[1].lerp(exit, t), t));
        }
    }
    if corners.len() > 1 {
        path.push(*corners.last().unwrap());
    }
    path.dedup();
    path
}

pub(crate) fn paint(
    painter: &Painter,
    points: &[Pos2],
    stroke: Stroke,
    zoom: f32,
    background: Color32,
    dashed: bool,
) {
    let path = rounded_path(points, 9.0 * zoom);
    if path.len() < 2 {
        return;
    }
    if dashed {
        painter.extend(Shape::dashed_line(&path, stroke, 5.0 * zoom, 4.0 * zoom));
        return;
    }
    // A narrow background halo makes crossings readable without adding junctions.
    painter.add(Shape::line(
        path.clone(),
        Stroke::new(stroke.width + 2.5 * zoom, background),
    ));
    painter.add(Shape::line(path, stroke));
    if let Some(segment) = points.windows(2).rfind(|pair| pair[0] != pair[1]) {
        let tip = segment[1];
        let direction = (tip - segment[0]).normalized();
        let normal = Vec2::new(-direction.y, direction.x);
        let length = (7.0 * zoom).min(segment[0].distance(tip));
        let base = tip - direction * length;
        painter.add(Shape::convex_polygon(
            vec![
                tip,
                base + normal * length * 0.43,
                base - normal * length * 0.43,
            ],
            stroke.color,
            Stroke::NONE,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_connections_keep_endpoints_and_fit_inside_each_turn() {
        let points = [
            Pos2::ZERO,
            Pos2::new(30.0, 0.0),
            Pos2::new(30.0, 8.0),
            Pos2::new(60.0, 8.0),
        ];
        let path = rounded_path(&points, 9.0);
        assert_eq!(path.first(), points.first());
        assert_eq!(path.last(), points.last());
        assert!(path.iter().all(|p| p.is_finite()
            && p.x >= 0.0
            && p.x <= 60.0
            && p.y >= 0.0
            && p.y <= 8.0));
        assert!(path.windows(2).all(|pair| pair[0].distance(pair[1]) > 0.0));
        assert!(path.iter().any(|p| p.x < 30.0 && p.y > 0.0 && p.y < 4.0));
        assert!(rounded_path(&[], 9.0).is_empty());
        assert_eq!(
            rounded_path(&[Pos2::ZERO, Pos2::ZERO], 9.0),
            vec![Pos2::ZERO]
        );
    }
}
