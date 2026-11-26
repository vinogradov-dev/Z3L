//! The pixels of a final image and the ray each sample takes.
//!
//! An image is addressed the way a file is written: pixel `(0, 0)` is the top-left corner, so a row
//! index grows downwards while a normalised device coordinate grows upwards. That single flip is the
//! oldest mistake a camera-into-image path makes, so it lives in one function, is tested against the
//! viewport's own projection, and is transcribed into WGSL rather than re-derived there.
//!
//! The ray itself is the viewport's: [`camera_ray`] hands the normalised coordinate to
//! `Camera::ray_ndc`, which is what picking uses. A final render is then, by construction, the view a
//! person was looking at, and a test asserts it at a known point rather than trusting it.
//!
//! A camera with a lens bends the same ray: [`camera_ray_through`] picks a point of the lens disc and
//! aims it at the point of the focal plane the pinhole ray would have met, which is the thin-lens
//! camera every renderer means by depth of field (Y5). The two are one function with a lens of radius
//! zero going through the pinhole branch, so a picture cannot differ between "no lens" and "a lens of
//! no radius".
//!
//! Refuses: a size of zero, a non-finite sample position or a broken aspect ratio is `None`, never a
//! ray of NaN that would trace an image of noise.

use geom::DVec2;
use scene::Camera;

use crate::ray::Ray;
use crate::sampler::concentric_disc;

/// The aspect ratio of an image, or `None` when its height is zero.
pub fn aspect(width: u32, height: u32) -> Option<f64> {
    if width == 0 || height == 0 {
        return None;
    }
    Some(f64::from(width) / f64::from(height))
}

/// The normalised device coordinate of a point given in image coordinates.
///
/// `(0, 0)` is the top-left corner of the image and `(width, height)` the bottom-right corner, so the
/// centre of pixel `(x, y)` is at `(x + 0.5, y + 0.5)`. The result is `(-1, -1)` at the bottom-left and
/// `(1, 1)` at the top-right, which is what the camera expects.
pub fn ndc_of_point(x: f64, y: f64, width: u32, height: u32) -> Option<DVec2> {
    let (width, height) = (f64::from(width), f64::from(height));
    if !x.is_finite() || !y.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(DVec2::new(x / width * 2.0 - 1.0, 1.0 - y / height * 2.0))
}

/// The ray through one sample of an image.
///
/// The satellite: `jitter` is the sample's offset from the pixel's centre, in pixel units, so a
/// sampler hands over a value inside `[-0.5, 0.5)` on each axis and the ray stays inside its pixel.
pub fn camera_ray(camera: Camera, jitter: DVec2, pixel: (u32, u32), width: u32, height: u32) -> Option<Ray> {
    camera_ray_through(camera, jitter, [0.5, 0.5], pixel, width, height)
}

/// The ray through one sample of an image, from one point of the camera's lens.
///
/// `lens` is a point of the unit square the sampler drew; it becomes a point of the lens disc, and the
/// ray runs from there to the point of the focal plane the pinhole ray would have met. A pinhole — or
/// an orthographic view, which has no lens — ignores the draw and takes the ray the viewport would
/// have taken, so the cost of a lens nobody asked for is one comparison (Y5).
pub fn camera_ray_through(
    camera: Camera,
    jitter: DVec2,
    lens: [f32; 2],
    pixel: (u32, u32),
    width: u32,
    height: u32,
) -> Option<Ray> {
    let ndc = ndc_of_point(f64::from(pixel.0) + 0.5 + jitter.x, f64::from(pixel.1) + 0.5 + jitter.y, width, height)?;
    let aspect = aspect(width, height)?;
    let ray = camera.ray_ndc(ndc.x, ndc.y, aspect);
    let (origin, direction) = (ray.origin(), ray.direction());
    if camera.is_pinhole() {
        return Ray::new(origin, direction);
    }
    let Some(distance) = focus_distance(camera, direction) else {
        return Ray::new(origin, direction);
    };
    let focal_point = origin + direction * distance;
    let disc = concentric_disc(lens);
    let lens_point = origin
        + camera.right() * (f64::from(disc[0]) * camera.aperture())
        + camera.up() * (f64::from(disc[1]) * camera.aperture());
    Ray::new(lens_point, focal_point - lens_point)
}

/// The distance along a ray at which it meets the plane the lens focuses on, or `None` for a view
/// whose rays never meet it.
///
/// The lens is a plane at the focus distance in front of the eye, so the distance along a ray is the
/// focus distance divided by the cosine between the ray and the view axis. An orthographic view has no
/// eye and no focal plane, and a ray parallel to the axis meets it at the focus distance itself.
fn focus_distance(camera: Camera, direction: geom::DVec3) -> Option<f64> {
    if matches!(camera.projection(), scene::Projection::Orthographic { .. }) {
        return None;
    }
    let cosine = direction.dot(camera.forward());
    if !cosine.is_finite() || cosine <= 1.0e-9 {
        return None;
    }
    let distance = camera.focus() / cosine;
    (distance.is_finite() && distance > 0.0).then_some(distance)
}

/// The diameter of the blur a point at `distance` gets from a lens, in millimetres of the world at
/// that distance.
///
/// Two rays from opposite sides of a lens of radius `a` converge on the focal plane and cross there, so
/// a point at `distance` is spread over `2 a |distance - focus| / focus`. It is the thin-lens circle of
/// confusion, and the device test measures the picture against it rather than trusting the algebra
/// (Y5).
pub fn circle_of_confusion(aperture: f64, focus: f64, distance: f64) -> f64 {
    if !aperture.is_finite() || !focus.is_finite() || !distance.is_finite() || focus <= 0.0 || aperture <= 0.0 {
        return 0.0;
    }
    2.0 * aperture * (distance - focus).abs() / focus
}

/// The diameter of that blur measured in image pixels, which is what a picture shows.
///
/// The image covers `2 · distance · tan(fov/2)` millimetres at the point's own depth over `height`
/// pixels, so the world blur divided by that scale is the blur a person counts. An orthographic view
/// has no depth of field and answers zero.
pub fn confusion_in_pixels(camera: Camera, distance: f64, height: u32) -> f64 {
    let scene::Projection::Perspective { fov_y } = camera.projection() else {
        return 0.0;
    };
    if height == 0 || !distance.is_finite() || distance <= 0.0 {
        return 0.0;
    }
    let covered = 2.0 * distance * (fov_y * 0.5).tan();
    if !covered.is_finite() || covered <= 0.0 {
        return 0.0;
    }
    circle_of_confusion(camera.aperture(), camera.focus(), distance) / covered * f64::from(height)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use geom::DVec3;
    use scene::Projection;

    use super::*;

    const WIDTH: u32 = 800;
    const HEIGHT: u32 = 600;

    fn camera() -> Camera {
        // A three-quarter view of a two-metre cabinet, the way the viewport opens.
        Camera::new(DVec3::new(0.0, 0.0, 1000.0), 3000.0).expect("a camera with a distance")
    }

    /// The image coordinate a normalised device coordinate lands on, the inverse of
    /// [`ndc_of_point`] written out independently so the test does not restate the implementation.
    fn image_point(ndc: DVec2) -> (f64, f64) {
        ((ndc.x + 1.0) * 0.5 * f64::from(WIDTH), (1.0 - ndc.y) * 0.5 * f64::from(HEIGHT))
    }

    /// How much of the world the image's full height covers at `depth`, in millimetres.
    fn visible_height(camera: Camera, depth: f64) -> f64 {
        match camera.projection() {
            scene::Projection::Perspective { fov_y } => 2.0 * depth * (fov_y * 0.5).tan(),
            scene::Projection::Orthographic { height } => height,
        }
    }

    #[test]
    fn a_pinhole_takes_the_same_ray_whatever_the_lens_draw_is() {
        // The clause about costing nothing: a lens of no radius and a viewport's pinhole are one ray,
        // so a picture cannot differ between them, and an orthographic view has no lens at all.
        let camera = camera();
        let pinhole = camera_ray(camera, DVec2::new(0.25, -0.25), (100, 200), WIDTH, HEIGHT).expect("a ray");
        for lens in [[0.0f32, 0.0], [1.0, 1.0], [0.5, 0.25]] {
            let same =
                camera_ray_through(camera, DVec2::new(0.25, -0.25), lens, (100, 200), WIDTH, HEIGHT).expect("a ray");
            assert_eq!(same.origin(), pinhole.origin(), "a pinhole does not move the eye");
            assert_eq!(same.direction(), pinhole.direction(), "and does not turn the ray");
        }
        let orthographic = camera.with_projection(Projection::Orthographic { height: 2_000.0 });
        let lensed = orthographic.with_lens(80.0, 1_500.0).expect("a lens");
        let flat = camera_ray(orthographic, DVec2::ZERO, (10, 10), WIDTH, HEIGHT).expect("a ray");
        let asked = camera_ray_through(lensed, DVec2::ZERO, [0.9, 0.1], (10, 10), WIDTH, HEIGHT).expect("a ray");
        assert_eq!(asked.origin(), flat.origin(), "an orthographic view has no lens to draw from");
        assert_eq!(asked.direction(), flat.direction());
    }

    #[test]
    fn a_lens_spreads_its_rays_and_keeps_the_focal_plane_pin_sharp() {
        // The thin-lens promise: every ray of one sample starts somewhere on the lens disc, and all of
        // them meet the focal plane at the point the pinhole ray would have reached, so a point there
        // is the one thing in the picture that never blurs.
        let camera = camera().with_lens(100.0, 2_000.0).expect("a lens");
        let pinhole = camera_ray(camera, DVec2::ZERO, (400, 300), WIDTH, HEIGHT).expect("a ray");
        let focal = pinhole.origin()
            + pinhole.direction() * focus_distance(camera, pinhole.direction()).expect("a focal plane");
        let mut offsets = Vec::new();
        for lens in [[0.0f32, 0.0], [1.0, 0.5], [0.5, 1.0], [0.5, 0.5]] {
            let ray = camera_ray_through(camera, DVec2::ZERO, lens, (400, 300), WIDTH, HEIGHT).expect("a ray");
            // The ray meets the focal plane where the pinhole ray did.
            let travel = (focal - ray.origin()).dot(ray.direction());
            let met = ray.origin() + ray.direction() * travel;
            assert!((met - focal).length() < 0.01, "{lens:?} meets the focal plane at {met:?}, not {focal:?}");
            let offset = ray.origin() - camera.eye();
            assert!((offset - camera.forward() * offset.dot(camera.forward())).length() <= 100.0 + 1.0e-9);
            offsets.push(offset.length());
        }
        assert!(offsets.iter().any(|offset| *offset < 1.0e-9), "the middle of the lens is the pinhole");
        assert!(offsets.iter().any(|offset| (*offset - 100.0).abs() < 0.01), "the edge is a radius away: {offsets:?}");
    }

    #[test]
    fn a_point_off_the_focal_plane_blurs_by_the_thin_lens_circle() {
        // The number the device test measures: a focus of two metres and a lens radius of a hundred
        // millimetres spreads a point at one metre by `2 * 100 * |1000 - 2000| / 2000` = 100 mm, and
        // the focal plane itself does not blur at all.
        assert_eq!(circle_of_confusion(100.0, 2_000.0, 1_000.0), 100.0);
        assert_eq!(circle_of_confusion(100.0, 2_000.0, 2_000.0), 0.0);
        assert_eq!(
            circle_of_confusion(100.0, 2_000.0, 4_000.0),
            200.0,
            "a point twice as far from the focus as the near one blurs twice as much"
        );
        assert_eq!(circle_of_confusion(0.0, 2_000.0, 1_000.0), 0.0, "a pinhole blurs nothing");
        assert_eq!(circle_of_confusion(f64::NAN, 2_000.0, 1_000.0), 0.0);
        assert_eq!(circle_of_confusion(100.0, 0.0, 1_000.0), 0.0);
        // In pixels: a view whose half-height is half the distance covers 1000 mm at the point's own
        // depth, so a 100 mm blur across 300 pixels is 30 of them.
        let camera = Camera::new(DVec3::ZERO, 3_000.0)
            .expect("a camera")
            .with_projection(Projection::Perspective { fov_y: 2.0 * (0.5f64).atan() })
            .with_lens(100.0, 2_000.0)
            .expect("a lens");
        let pixels = confusion_in_pixels(camera, 1_000.0, 300);
        assert!((pixels - 30.0).abs() < 1.0e-3, "{pixels}");
        // The focal plane itself is sharp in pixels too, and an orthographic view has no depth at all.
        assert_eq!(confusion_in_pixels(camera, 2_000.0, 300), 0.0);
        let flat = camera.with_projection(Projection::Orthographic { height: 300.0 }).with_lens(100.0, 2_000.0);
        assert_eq!(confusion_in_pixels(flat.expect("a lens"), 1_000.0, 300), 0.0);
    }

    #[test]
    fn the_mapping_is_the_inverse_of_the_viewports_projection() {
        let camera = camera();
        let aspect = aspect(WIDTH, HEIGHT).expect("an aspect ratio");
        for point in [DVec3::new(0.0, 0.0, 0.0), DVec3::new(800.0, 250.0, 120.0), DVec3::new(-900.0, -400.0, -60.0)] {
            let ndc = camera.project_ndc(point, aspect).expect("the point is in front of the camera");
            let (x, y) = image_point(ndc);
            let back = ndc_of_point(x, y, WIDTH, HEIGHT).expect("the image point maps");
            assert!((back.x - ndc.x).abs() < 1e-12 && (back.y - ndc.y).abs() < 1e-12, "{back:?} vs {ndc:?}");
        }
    }

    #[test]
    fn pixel_zero_zero_is_the_top_left_corner() {
        let camera = camera();
        let top_left = camera_ray(camera, DVec2::ZERO, (0, 0), WIDTH, HEIGHT).expect("a ray");
        let bottom_right = camera_ray(camera, DVec2::ZERO, (WIDTH - 1, HEIGHT - 1), WIDTH, HEIGHT).expect("a ray");
        // Orientation is read from the camera itself, so the test survives a change of default view.
        let (right, up) = (camera.right(), camera.up());
        let to_top_left = top_left.direction() - camera.forward();
        let to_bottom_right = bottom_right.direction() - camera.forward();
        assert!(to_top_left.dot(up) > 0.0, "the first row is above the centre: {to_top_left:?}");
        assert!(to_top_left.dot(right) < 0.0, "the first column is left of the centre: {to_top_left:?}");
        assert!(to_bottom_right.dot(up) < 0.0, "{to_bottom_right:?}");
        assert!(to_bottom_right.dot(right) > 0.0, "{to_bottom_right:?}");
    }

    #[test]
    fn a_pixels_ray_lands_within_one_pixel_of_a_point_the_viewport_projects() {
        let camera = camera();
        let aspect = aspect(WIDTH, HEIGHT).expect("an aspect ratio");
        let point = DVec3::new(420.0, 310.0, 60.0);
        let ndc = camera.project_ndc(point, aspect).expect("the point is visible");
        let (x, y) = image_point(ndc);
        let pixel = (
            (x - 0.5).round().clamp(0.0, f64::from(WIDTH - 1)) as u32,
            (y - 0.5).round().clamp(0.0, f64::from(HEIGHT - 1)) as u32,
        );
        let ray = camera_ray(camera, DVec2::ZERO, pixel, WIDTH, HEIGHT).expect("a ray");
        // The distance from the pixel's ray to the point, measured on the plane through the point
        // that faces the camera, is shorter than one pixel's width on that plane.
        let plane_depth = (point - camera.eye()).dot(camera.forward());
        let pixel_width = visible_height(camera, plane_depth) * aspect / f64::from(WIDTH);
        let along = (point - ray.origin()).dot(camera.forward()) / ray.direction().dot(camera.forward());
        let on_ray = ray.at(along);
        let gap = (on_ray - point).length();
        assert!(gap <= pixel_width, "the point is {gap} mm from the ray, and a pixel is {pixel_width} mm there");
    }

    #[test]
    fn a_jittered_sample_stays_inside_its_own_pixel() {
        let centre = ndc_of_point(100.5, 200.5, WIDTH, HEIGHT).expect("the centre maps");
        let left = ndc_of_point(100.0, 200.0, WIDTH, HEIGHT).expect("the corner maps");
        let right = ndc_of_point(101.0, 201.0, WIDTH, HEIGHT).expect("the corner maps");
        assert!(left.x < centre.x && centre.x < right.x, "the pixel spans its edges");
        assert!(left.y > centre.y && centre.y > right.y, "the row grows downwards");
        for jitter in [DVec2::new(-0.5, -0.5), DVec2::new(0.5, 0.5), DVec2::new(-0.5, 0.5)] {
            let sample = ndc_of_point(100.5 + jitter.x, 200.5 + jitter.y, WIDTH, HEIGHT).expect("the sample maps");
            assert!(sample.x >= left.x && sample.x <= right.x, "{jitter:?} left the pixel");
            assert!(sample.y <= left.y && sample.y >= right.y, "{jitter:?} left the pixel");
        }
    }

    #[test]
    fn an_orthographic_view_takes_parallel_rays_from_different_origins() {
        let mut camera = camera();
        camera.set_orthographic();
        let first = camera_ray(camera, DVec2::ZERO, (10, 10), WIDTH, HEIGHT).expect("a ray");
        let second = camera_ray(camera, DVec2::ZERO, (700, 400), WIDTH, HEIGHT).expect("a ray");
        assert!((first.direction() - second.direction()).length() < 1e-12, "an orthographic view is parallel");
        assert!((first.origin() - second.origin()).length() > 1.0, "each pixel starts somewhere else");
    }

    #[test]
    fn a_broken_size_or_a_broken_coordinate_is_no_image_point() {
        assert!(aspect(0, 100).is_none());
        assert!(aspect(100, 0).is_none());
        assert!(ndc_of_point(1.0, 1.0, 0, 100).is_none());
        assert!(ndc_of_point(f64::NAN, 1.0, 100, 100).is_none());
        assert!(camera_ray(camera(), DVec2::ZERO, (0, 0), 0, 0).is_none());
    }
}
