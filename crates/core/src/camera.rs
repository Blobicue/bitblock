//! Observer-only camera state and small, dependency-free 3D math.
//!
//! World coordinates are deliberately represented as `f64` for checkpoint 1. Keeping every
//! absolute-coordinate operation behind this module leaves room for a later chunk-relative
//! representation without coupling the renderer or input code to that choice.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_3, PI, TAU};
use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

/// The default vertical field of view: 60 degrees.
pub const DEFAULT_VERTICAL_FOV_RADIANS: f64 = FRAC_PI_3;

/// Mouse radians applied per raw device-motion unit.
pub const DEFAULT_MOUSE_SENSITIVITY: f64 = 0.0025;

/// The logarithmic distance change per mouse-wheel line.
pub const DEFAULT_DOLLY_LOG_STEP: f64 = 0.15;

/// Smallest supported distance between the observer and its observation anchor.
pub const MIN_OBSERVATION_DISTANCE: f64 = 1.0e-9;

/// Largest checkpoint-1 distance. This is intentionally finite so all camera state stays finite.
pub const MAX_OBSERVATION_DISTANCE: f64 = 1.0e150;

/// Observer spawn: the center of the empty voxel immediately on the origin's -Z side.
pub const OBSERVER_SPAWN_POSITION: Vec3 = Vec3::new(0.0, 0.0, -1.0);

const PITCH_MARGIN_RADIANS: f64 = 1.0e-6;
/// Largest magnitude retained by the observer's clamped pitch state.
pub const MAX_ABS_PITCH_RADIANS: f64 = FRAC_PI_2 - PITCH_MARGIN_RADIANS;
pub(crate) const ONE_PIXEL_EPSILON: f64 = 1.0e-9;

/// A world-space vector or point.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub const fn splat(value: f64) -> Self {
        Self::new(value, value, value)
    }

    pub fn dot(self, rhs: Self) -> f64 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    pub fn try_normalized(self) -> Option<Self> {
        let length = self.length();
        (length.is_finite() && length > 0.0).then(|| self / length)
    }

    pub fn normalized(self) -> Self {
        self.try_normalized()
            .expect("cannot normalize a zero-length or non-finite vector")
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul<f64> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Mul<Vec3> for f64 {
    type Output = Vec3;

    fn mul(self, rhs: Vec3) -> Self::Output {
        rhs * self
    }
}

impl MulAssign<f64> for Vec3 {
    fn mul_assign(&mut self, rhs: f64) {
        *self = *self * rhs;
    }
}

impl Div<f64> for Vec3 {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

impl Neg for Vec3 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y, -self.z)
    }
}

/// Orthonormal camera directions in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraBasis {
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
}

/// A world-space ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

/// Perspective projection in physical framebuffer coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectedPoint {
    pub x: f64,
    pub y: f64,
    pub depth: f64,
}

/// Immutable camera and viewport state shared by every projection in one render.
///
/// It preserves the arithmetic in [`Camera::project`] while avoiding repeated basis and focal
/// setup for each cube vertex.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CameraProjection {
    position: Vec3,
    basis: CameraBasis,
    focal_length: f64,
    half_width: f64,
    half_height: f64,
}

#[cfg(test)]
thread_local! {
    static PROJECTION_CONTEXT_BUILD_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_projection_context_build_count() {
    PROJECTION_CONTEXT_BUILD_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn projection_context_build_count() -> usize {
    PROJECTION_CONTEXT_BUILD_COUNT.with(std::cell::Cell::get)
}

impl CameraProjection {
    pub(crate) fn position(self) -> Vec3 {
        self.position
    }

    /// The depth [`Self::project`] would report for `point`, without the screen coordinates.
    ///
    /// It evaluates the same expression on the same operands, so it agrees bit for bit with
    /// the `depth` field of a successful projection. Callers that only need a traversal
    /// order pay one dot product instead of a full projection.
    pub(crate) fn view_depth(self, point: Vec3) -> f64 {
        let relative = point - self.position;
        relative.dot(self.basis.forward)
    }

    pub(crate) fn project(self, point: Vec3) -> Option<ProjectedPoint> {
        if !point.is_finite() {
            return None;
        }

        let relative = point - self.position;
        let depth = relative.dot(self.basis.forward);
        if !depth.is_finite() || depth <= 0.0 {
            return None;
        }

        let x = self.half_width + self.focal_length * relative.dot(self.basis.right) / depth;
        let y = self.half_height - self.focal_length * relative.dot(self.basis.up) / depth;

        (x.is_finite() && y.is_finite()).then_some(ProjectedPoint { x, y, depth })
    }

    pub(crate) fn bounds_fit_one_pixel(self, min: Vec3, max: Vec3) -> bool {
        if !bounds_are_valid(min, max) {
            return false;
        }

        bounds_corners(min, max).into_iter().all(|corner| {
            self.project(corner).is_some_and(|projected| {
                (projected.x - self.half_width).abs() <= 0.5 + ONE_PIXEL_EPSILON
                    && (projected.y - self.half_height).abs() <= 0.5 + ONE_PIXEL_EPSILON
            })
        })
    }
}

/// Free observer state. It has no body, collision volume, gravity, or other entity semantics.
#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    position: Vec3,
    yaw_radians: f64,
    pitch_radians: f64,
    observation_anchor: Vec3,
    observation_distance: f64,
    vertical_fov_radians: f64,
}

impl Camera {
    /// Creates a camera at `position`; its anchor lies `observation_distance` down the center ray.
    pub fn from_position(
        position: Vec3,
        yaw_radians: f64,
        pitch_radians: f64,
        observation_distance: f64,
        vertical_fov_radians: f64,
    ) -> Self {
        validate_camera_inputs(
            position,
            yaw_radians,
            pitch_radians,
            observation_distance,
            vertical_fov_radians,
        );

        let mut camera = Self {
            position,
            yaw_radians: wrap_yaw(yaw_radians),
            pitch_radians: clamp_pitch(pitch_radians),
            observation_anchor: Vec3::ZERO,
            observation_distance,
            vertical_fov_radians,
        };
        camera.observation_anchor = camera.position + camera.forward() * observation_distance;
        camera
    }

    /// Creates a camera looking through `anchor` from `observation_distance` away.
    pub fn from_anchor(
        anchor: Vec3,
        observation_distance: f64,
        yaw_radians: f64,
        pitch_radians: f64,
        vertical_fov_radians: f64,
    ) -> Self {
        assert!(anchor.is_finite(), "camera anchor must be finite");
        let mut camera = Self::from_position(
            anchor,
            yaw_radians,
            pitch_radians,
            observation_distance,
            vertical_fov_radians,
        );
        camera.observation_anchor = anchor;
        camera.position = anchor - camera.forward() * observation_distance;
        camera
    }

    /// Creates the observer beside the origin voxel, looking directly at its center.
    ///
    /// The observer has no body or collision semantics. This location is simply the center of
    /// coordinate `(0, 0, -1)`, the empty voxel adjacent to the origin bedrock's -Z face.
    pub fn for_observer_spawn() -> Self {
        let camera = Self::from_anchor(Vec3::ZERO, 1.0, 0.0, 0.0, DEFAULT_VERTICAL_FOV_RADIANS);
        debug_assert_eq!(camera.position, OBSERVER_SPAWN_POSITION);
        camera
    }

    /// Initial checkpoint camera. The origin voxel occupies `[-0.5, 0.5]^3`.
    ///
    /// Its near face projects to exactly one physical pixel in the requested square viewport.
    pub fn for_single_pixel_voxel(viewport_size: u32) -> Self {
        assert!(viewport_size > 0, "viewport must contain physical pixels");
        let mut camera = Self::from_anchor(Vec3::ZERO, 1.0, 0.0, 0.0, DEFAULT_VERTICAL_FOV_RADIANS);
        camera.fit_bounds(
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
            viewport_size,
            viewport_size,
        );
        camera
    }

    pub fn position(&self) -> Vec3 {
        self.position
    }

    pub fn observation_anchor(&self) -> Vec3 {
        self.observation_anchor
    }

    pub fn observation_distance(&self) -> f64 {
        self.observation_distance
    }

    pub fn yaw_radians(&self) -> f64 {
        self.yaw_radians
    }

    pub fn pitch_radians(&self) -> f64 {
        self.pitch_radians
    }

    pub fn vertical_fov_radians(&self) -> f64 {
        self.vertical_fov_radians
    }

    pub fn basis(&self) -> CameraBasis {
        let (sin_yaw, cos_yaw) = self.yaw_radians.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch_radians.sin_cos();

        let forward = Vec3::new(sin_yaw * cos_pitch, sin_pitch, cos_yaw * cos_pitch);
        let right = Vec3::new(cos_yaw, 0.0, -sin_yaw);
        let up = forward.cross(right);

        CameraBasis { forward, right, up }
    }

    pub fn forward(&self) -> Vec3 {
        self.basis().forward
    }

    pub fn right(&self) -> Vec3 {
        self.basis().right
    }

    pub fn up(&self) -> Vec3 {
        self.basis().up
    }

    /// Applies raw mouse motion. Positive X turns right; positive Y turns down.
    ///
    /// Mouse look rotates in place and moves the observation anchor onto the new center ray.
    pub fn apply_mouse_delta(&mut self, delta_x: f64, delta_y: f64, sensitivity: f64) {
        assert!(
            delta_x.is_finite() && delta_y.is_finite() && sensitivity.is_finite(),
            "mouse input must be finite"
        );
        assert!(sensitivity >= 0.0, "mouse sensitivity cannot be negative");
        self.rotate(delta_x * sensitivity, -delta_y * sensitivity);
    }

    /// Adds yaw and pitch in radians while keeping the observer position fixed.
    pub fn rotate(&mut self, yaw_delta: f64, pitch_delta: f64) {
        assert!(
            yaw_delta.is_finite() && pitch_delta.is_finite(),
            "camera rotation must be finite"
        );
        self.yaw_radians = wrap_yaw(self.yaw_radians + yaw_delta);
        self.pitch_radians = clamp_pitch(self.pitch_radians + pitch_delta);
        self.observation_anchor = self.position + self.forward() * self.observation_distance;
    }

    /// Moves observer and anchor together. Local X is right, Y is up, and Z is forward.
    pub fn translate(&mut self, local_delta: Vec3) {
        assert!(local_delta.is_finite(), "camera translation must be finite");
        let basis = self.basis();
        let world_delta =
            basis.right * local_delta.x + basis.up * local_delta.y + basis.forward * local_delta.z;
        self.translate_world(world_delta);
    }

    /// Moves observer and anchor together by an absolute world-space delta.
    pub fn translate_world(&mut self, world_delta: Vec3) {
        assert!(world_delta.is_finite(), "camera translation must be finite");
        let position = self.position + world_delta;
        let anchor = self.observation_anchor + world_delta;
        assert!(
            position.is_finite() && anchor.is_finite(),
            "camera translation exceeded checkpoint-1 coordinate range"
        );
        self.position = position;
        self.observation_anchor = anchor;
    }

    /// Moves logarithmically around the fixed observation anchor.
    ///
    /// Positive wheel values move toward the anchor. Each wheel unit multiplies distance by
    /// `exp(-DEFAULT_DOLLY_LOG_STEP)`.
    pub fn dolly(&mut self, wheel_delta: f64) -> f64 {
        self.dolly_clamped(
            wheel_delta,
            DEFAULT_DOLLY_LOG_STEP,
            MIN_OBSERVATION_DISTANCE,
            MAX_OBSERVATION_DISTANCE,
        )
    }

    /// Logarithmic dolly with explicit speed and distance bounds.
    ///
    /// This is the hook used to clamp wheel-out at the current finite world's one-pixel fit
    /// distance, while retaining a separately chosen close-view limit.
    pub fn dolly_clamped(
        &mut self,
        wheel_delta: f64,
        log_step: f64,
        min_distance: f64,
        max_distance: f64,
    ) -> f64 {
        assert!(
            wheel_delta.is_finite() && log_step.is_finite(),
            "dolly input must be finite"
        );
        assert!(log_step >= 0.0, "dolly logarithmic step cannot be negative");
        validate_distance(min_distance);
        validate_distance(max_distance);
        assert!(min_distance <= max_distance, "invalid dolly distance range");

        let log_distance = self.observation_distance.ln() - wheel_delta * log_step;
        let distance = log_distance
            .clamp(min_distance.ln(), max_distance.ln())
            .exp();
        self.set_observation_distance(distance)
    }

    /// Changes distance while keeping the anchor and orientation fixed.
    pub fn set_observation_distance(&mut self, distance: f64) -> f64 {
        validate_distance(distance);
        let position = self.observation_anchor - self.forward() * distance;
        assert!(
            position.is_finite(),
            "observation distance exceeded checkpoint-1 coordinate range"
        );
        self.observation_distance = distance;
        self.position = position;
        distance
    }

    /// Replaces the anchor and moves the observer to retain orientation and distance.
    pub fn set_observation_anchor(&mut self, anchor: Vec3) {
        assert!(anchor.is_finite(), "camera anchor must be finite");
        let position = anchor - self.forward() * self.observation_distance;
        assert!(
            position.is_finite(),
            "camera anchor exceeds coordinate range"
        );
        self.observation_anchor = anchor;
        self.position = position;
    }

    /// Perspective focal length in physical pixels for a viewport height.
    pub fn viewport_focal_length(&self, viewport_height: u32) -> f64 {
        assert!(viewport_height > 0, "viewport must contain physical pixels");
        f64::from(viewport_height) / (2.0 * (self.vertical_fov_radians * 0.5).tan())
    }

    /// Captures the invariant setup shared by projections in one nonzero viewport.
    pub(crate) fn projection_for_viewport(
        &self,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Option<CameraProjection> {
        if viewport_width == 0 || viewport_height == 0 {
            return None;
        }
        #[cfg(test)]
        PROJECTION_CONTEXT_BUILD_COUNT.with(|count| count.set(count.get() + 1));
        Some(CameraProjection {
            position: self.position,
            basis: self.basis(),
            focal_length: self.viewport_focal_length(viewport_height),
            half_width: f64::from(viewport_width) * 0.5,
            half_height: f64::from(viewport_height) * 0.5,
        })
    }

    pub fn center_ray(&self) -> Ray {
        Ray {
            origin: self.position,
            direction: self.forward(),
        }
    }

    /// Returns a ray through a physical framebuffer coordinate.
    pub fn ray_for_pixel(
        &self,
        pixel_x: f64,
        pixel_y: f64,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Option<Ray> {
        if viewport_width == 0
            || viewport_height == 0
            || !pixel_x.is_finite()
            || !pixel_y.is_finite()
        {
            return None;
        }

        let focal_length = self.viewport_focal_length(viewport_height);
        let basis = self.basis();
        let camera_x = (pixel_x - f64::from(viewport_width) * 0.5) / focal_length;
        let camera_y = -(pixel_y - f64::from(viewport_height) * 0.5) / focal_length;
        let direction = (basis.forward + basis.right * camera_x + basis.up * camera_y).normalized();

        Some(Ray {
            origin: self.position,
            direction,
        })
    }

    /// Projects a world point into physical framebuffer coordinates.
    ///
    /// Points behind or exactly on the camera plane return `None`. Coordinates outside the
    /// viewport remain valid projections so callers can perform their own clipping.
    pub fn project(
        &self,
        point: Vec3,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Option<ProjectedPoint> {
        if viewport_width == 0 || viewport_height == 0 || !point.is_finite() {
            return None;
        }

        self.projection_for_viewport(viewport_width, viewport_height)
            .expect("nonzero projection viewport must produce a projection context")
            .project(point)
    }

    /// Distance from the current anchor at which `bounds` fit inside the central physical pixel.
    ///
    /// The value is exact for the eight corners under perspective projection. It can be supplied
    /// as `max_distance` to [`Self::dolly_clamped`] to enforce dynamic top saturation.
    pub fn fit_distance_for_bounds(
        &self,
        min: Vec3,
        max: Vec3,
        viewport_width: u32,
        viewport_height: u32,
    ) -> f64 {
        validate_viewport(viewport_width, viewport_height);
        self.fit_distance_from_anchor(min, max, self.observation_anchor, viewport_height)
    }

    /// Re-anchors to the AABB center and makes the complete bounds fill exactly one central pixel.
    pub fn fit_bounds(
        &mut self,
        min: Vec3,
        max: Vec3,
        viewport_width: u32,
        viewport_height: u32,
    ) -> f64 {
        validate_bounds(min, max);
        validate_viewport(viewport_width, viewport_height);
        let anchor = min + (max - min) * 0.5;
        let distance = self.fit_distance_from_anchor(min, max, anchor, viewport_height);
        self.observation_anchor = anchor;
        self.observation_distance = distance;
        self.position = anchor - self.forward() * distance;
        assert!(
            self.position.is_finite(),
            "fitted camera position is not finite"
        );
        distance
    }

    /// Whether every corner of `bounds` is contained by the central physical pixel.
    ///
    /// `true` means the camera is at or beyond top saturation. A wheel-out path should clamp at
    /// the distance returned by [`Self::fit_distance_for_bounds`] rather than proceeding farther.
    pub fn bounds_fit_one_pixel(
        &self,
        min: Vec3,
        max: Vec3,
        viewport_width: u32,
        viewport_height: u32,
    ) -> bool {
        if viewport_width == 0 || viewport_height == 0 || !bounds_are_valid(min, max) {
            return false;
        }

        self.projection_for_viewport(viewport_width, viewport_height)
            .expect("nonzero projection viewport must produce a projection context")
            .bounds_fit_one_pixel(min, max)
    }

    fn fit_distance_from_anchor(
        &self,
        min: Vec3,
        max: Vec3,
        anchor: Vec3,
        viewport_height: u32,
    ) -> f64 {
        validate_bounds(min, max);
        assert!(anchor.is_finite(), "fit anchor must be finite");
        let basis = self.basis();
        let focal_length = self.viewport_focal_length(viewport_height);
        let mut required_distance = MIN_OBSERVATION_DISTANCE;

        for corner in bounds_corners(min, max) {
            let relative = corner - anchor;
            let camera_x = relative.dot(basis.right);
            let camera_y = relative.dot(basis.up);
            let camera_z = relative.dot(basis.forward);

            // |f*x/(distance+z)| <= 0.5 and likewise for Y. Rearranging each inequality
            // gives the exact minimum distance for this corner. The final term keeps every
            // corner strictly in front of the camera plane, including degenerate bounds.
            required_distance = required_distance
                .max(2.0 * focal_length * camera_x.abs() - camera_z)
                .max(2.0 * focal_length * camera_y.abs() - camera_z)
                .max(-camera_z + MIN_OBSERVATION_DISTANCE);
        }

        assert!(
            required_distance.is_finite() && required_distance <= MAX_OBSERVATION_DISTANCE,
            "bounds exceed checkpoint-1 camera coordinate range"
        );
        required_distance
    }
}

fn validate_camera_inputs(
    position: Vec3,
    yaw_radians: f64,
    pitch_radians: f64,
    observation_distance: f64,
    vertical_fov_radians: f64,
) {
    assert!(position.is_finite(), "camera position must be finite");
    assert!(
        yaw_radians.is_finite() && pitch_radians.is_finite(),
        "camera angles must be finite"
    );
    validate_distance(observation_distance);
    assert!(
        vertical_fov_radians.is_finite() && vertical_fov_radians > 0.0 && vertical_fov_radians < PI,
        "vertical field of view must be between zero and pi"
    );
}

fn validate_distance(distance: f64) {
    assert!(
        distance.is_finite()
            && (MIN_OBSERVATION_DISTANCE..=MAX_OBSERVATION_DISTANCE).contains(&distance),
        "observation distance is outside the finite checkpoint-1 range"
    );
}

fn validate_viewport(width: u32, height: u32) {
    assert!(
        width > 0 && height > 0,
        "viewport must contain physical pixels"
    );
}

fn validate_bounds(min: Vec3, max: Vec3) {
    assert!(bounds_are_valid(min, max), "invalid or non-finite bounds");
}

fn bounds_are_valid(min: Vec3, max: Vec3) -> bool {
    min.is_finite() && max.is_finite() && min.x <= max.x && min.y <= max.y && min.z <= max.z
}

fn bounds_corners(min: Vec3, max: Vec3) -> [Vec3; 8] {
    [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, max.y, max.z),
    ]
}

fn wrap_yaw(yaw_radians: f64) -> f64 {
    (yaw_radians + PI).rem_euclid(TAU) - PI
}

fn clamp_pitch(pitch_radians: f64) -> f64 {
    pitch_radians.clamp(-MAX_ABS_PITCH_RADIANS, MAX_ABS_PITCH_RADIANS)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1.0e-10;

    fn assert_close(actual: f64, expected: f64) {
        let tolerance = EPSILON * expected.abs().max(1.0);
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected:.15}, got {actual:.15}"
        );
    }

    fn assert_vec_close(actual: Vec3, expected: Vec3) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
        assert_close(actual.z, expected.z);
    }

    #[test]
    fn vec3_operations_are_consistent() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(-4.0, 5.0, -6.0);
        assert_eq!(a + b, Vec3::new(-3.0, 7.0, -3.0));
        assert_eq!(a - b, Vec3::new(5.0, -3.0, 9.0));
        assert_close(a.dot(b), -12.0);
        assert_eq!(Vec3::X.cross(Vec3::Y), Vec3::Z);
        assert_vec_close(
            Vec3::new(3.0, 4.0, 0.0).normalized(),
            Vec3::new(0.6, 0.8, 0.0),
        );
        assert!(Vec3::ZERO.try_normalized().is_none());
    }

    #[test]
    fn initial_origin_voxel_near_face_is_exactly_one_pixel() {
        let viewport = 1_024;
        let camera = Camera::for_single_pixel_voxel(viewport);
        let left = camera
            .project(Vec3::new(-0.5, 0.0, -0.5), viewport, viewport)
            .unwrap();
        let right = camera
            .project(Vec3::new(0.5, 0.0, -0.5), viewport, viewport)
            .unwrap();
        let top = camera
            .project(Vec3::new(0.0, 0.5, -0.5), viewport, viewport)
            .unwrap();
        let bottom = camera
            .project(Vec3::new(0.0, -0.5, -0.5), viewport, viewport)
            .unwrap();

        assert_close(right.x - left.x, 1.0);
        assert_close(bottom.y - top.y, 1.0);
        assert_vec_close(camera.observation_anchor(), Vec3::ZERO);
        assert_close(
            camera.observation_distance(),
            camera.viewport_focal_length(viewport) + 0.5,
        );
        assert!(camera.bounds_fit_one_pixel(
            Vec3::splat(-0.5),
            Vec3::splat(0.5),
            viewport,
            viewport
        ));
    }

    #[test]
    fn observer_spawn_is_in_the_negative_z_neighbor_facing_origin() {
        let camera = Camera::for_observer_spawn();

        assert_vec_close(camera.position(), OBSERVER_SPAWN_POSITION);
        assert_vec_close(camera.observation_anchor(), Vec3::ZERO);
        assert_vec_close(camera.forward(), Vec3::Z);
        assert_close(camera.observation_distance(), 1.0);

        let projected_origin = camera.project(Vec3::ZERO, 800, 600).unwrap();
        assert_close(projected_origin.x, 400.0);
        assert_close(projected_origin.y, 300.0);
        assert_close(projected_origin.depth, 1.0);
    }

    #[test]
    fn camera_basis_stays_orthonormal() {
        let camera = Camera::from_anchor(Vec3::ZERO, 10.0, 1.1, -0.7, FRAC_PI_3);
        let basis = camera.basis();

        assert_close(basis.forward.length(), 1.0);
        assert_close(basis.right.length(), 1.0);
        assert_close(basis.up.length(), 1.0);
        assert_close(basis.forward.dot(basis.right), 0.0);
        assert_close(basis.forward.dot(basis.up), 0.0);
        assert_close(basis.right.dot(basis.up), 0.0);
        assert_vec_close(basis.forward.cross(basis.right), basis.up);
    }

    #[test]
    fn mouse_look_rotates_in_place_and_clamps_pitch() {
        let mut camera = Camera::from_position(Vec3::new(4.0, 5.0, 6.0), 0.0, 0.0, 8.0, FRAC_PI_3);
        let original_position = camera.position();

        camera.apply_mouse_delta(100.0, -10_000.0, 0.01);

        assert_vec_close(camera.position(), original_position);
        assert!(camera.pitch_radians() < FRAC_PI_2);
        assert_close(
            (camera.observation_anchor() - camera.position()).length(),
            camera.observation_distance(),
        );
        assert_vec_close(
            (camera.observation_anchor() - camera.position()).normalized(),
            camera.forward(),
        );
    }

    #[test]
    fn translation_uses_camera_local_axes_and_moves_anchor_with_observer() {
        let mut camera = Camera::from_anchor(Vec3::ZERO, 5.0, FRAC_PI_2, 0.0, FRAC_PI_3);
        let position_before = camera.position();
        let anchor_before = camera.observation_anchor();
        let expected_delta = camera.right() * 2.0 + camera.up() * 3.0 + camera.forward() * 4.0;

        camera.translate(Vec3::new(2.0, 3.0, 4.0));

        assert_vec_close(camera.position(), position_before + expected_delta);
        assert_vec_close(camera.observation_anchor(), anchor_before + expected_delta);
    }

    #[test]
    fn dolly_is_logarithmic_and_keeps_anchor_fixed() {
        let mut camera = Camera::from_anchor(Vec3::new(2.0, 3.0, 4.0), 100.0, 0.3, 0.2, FRAC_PI_3);
        let anchor = camera.observation_anchor();

        camera.dolly_clamped(2.0, 0.25, 1.0, 1_000.0);
        assert_close(camera.observation_distance(), 100.0 * (-0.5_f64).exp());
        assert_vec_close(camera.observation_anchor(), anchor);

        camera.dolly_clamped(-2.0, 0.25, 1.0, 1_000.0);
        assert_close(camera.observation_distance(), 100.0);
        assert_vec_close(camera.observation_anchor(), anchor);

        camera.dolly_clamped(-1_000.0, 0.25, 1.0, 120.0);
        assert_close(camera.observation_distance(), 120.0);
    }

    #[test]
    fn center_ray_and_projection_are_inverse_at_screen_center() {
        let camera = Camera::from_position(Vec3::new(2.0, -4.0, 1.0), 0.8, -0.3, 9.0, FRAC_PI_3);
        let ray = camera.center_ray();
        let point = ray.origin + ray.direction * 17.0;
        let projected = camera.project(point, 800, 600).unwrap();

        assert_close(projected.x, 400.0);
        assert_close(projected.y, 300.0);
        assert_close(projected.depth, 17.0);
        assert_vec_close(
            camera
                .ray_for_pixel(400.0, 300.0, 800, 600)
                .unwrap()
                .direction,
            ray.direction,
        );
        assert!(
            camera
                .project(ray.origin - ray.direction, 800, 600)
                .is_none()
        );
    }

    #[test]
    fn projection_context_matches_public_projection_bit_for_bit() {
        let camera =
            Camera::from_position(Vec3::new(2.0, -4.0, 1.0), 0.8125, -0.4375, 9.0, FRAC_PI_3);
        let forward = camera.forward();
        let right = camera.right();
        let up = camera.up();
        let points = [
            camera.position() + forward * 17.0,
            camera.position() + forward * 17.0 + right * 63.0 - up * 41.0,
            camera.position() - forward,
            Vec3::new(1.0e12, -1.0e12, 5.0e11),
        ];

        for (width, height) in [(1, 1), (801, 801), (1_201, 801), (817, 509)] {
            let context = camera.projection_for_viewport(width, height).unwrap();
            for point in points {
                let direct = camera.project(point, width, height).map(|projected| {
                    [
                        projected.x.to_bits(),
                        projected.y.to_bits(),
                        projected.depth.to_bits(),
                    ]
                });
                let cached = context.project(point).map(|projected| {
                    [
                        projected.x.to_bits(),
                        projected.y.to_bits(),
                        projected.depth.to_bits(),
                    ]
                });
                assert_eq!(cached, direct, "viewport {width}x{height}, point {point:?}");
            }

            let min = Vec3::new(-8.0, 2.0, -3.0);
            let max = Vec3::new(5.0, 9.0, 11.0);
            assert_eq!(
                context.bounds_fit_one_pixel(min, max),
                camera.bounds_fit_one_pixel(min, max, width, height)
            );
        }

        assert!(camera.projection_for_viewport(0, 1).is_none());
        assert!(camera.projection_for_viewport(1, 0).is_none());
        assert!(camera.project(Vec3::ZERO, 0, 1).is_none());
        assert!(
            camera
                .project(Vec3::new(f64::NAN, 0.0, 0.0), 1, 1)
                .is_none()
        );
    }

    #[test]
    fn fit_bounds_handles_oriented_nonsquare_aabbs_exactly() {
        let mut camera = Camera::from_anchor(Vec3::ZERO, 2.0, 0.71, -0.38, FRAC_PI_3);
        let min = Vec3::new(-8.0, 2.0, -3.0);
        let max = Vec3::new(5.0, 9.0, 11.0);
        let width = 1_280;
        let height = 720;

        let distance = camera.fit_bounds(min, max, width, height);
        assert_close(
            distance,
            camera.fit_distance_for_bounds(min, max, width, height),
        );
        assert_vec_close(camera.observation_anchor(), min + (max - min) * 0.5);
        assert!(camera.bounds_fit_one_pixel(min, max, width, height));

        let center_x = f64::from(width) * 0.5;
        let center_y = f64::from(height) * 0.5;
        let edge = bounds_corners(min, max)
            .into_iter()
            .map(|corner| camera.project(corner, width, height).unwrap())
            .map(|point| (point.x - center_x).abs().max((point.y - center_y).abs()))
            .fold(0.0, f64::max);
        assert_close(edge, 0.5);

        camera.set_observation_distance(distance * 0.9);
        assert!(!camera.bounds_fit_one_pixel(min, max, width, height));
    }

    #[test]
    fn fit_distance_uses_current_anchor_for_dynamic_top_clamp() {
        let viewport = 600;
        let min = Vec3::new(-2.0, -1.0, -0.5);
        let max = Vec3::new(3.0, 4.0, 2.0);
        let mut camera = Camera::from_anchor(Vec3::new(10.0, 0.0, 0.0), 1.0, 0.0, 0.0, FRAC_PI_3);
        let top = camera.fit_distance_for_bounds(min, max, viewport, viewport);

        camera.set_observation_distance(top);
        assert!(camera.bounds_fit_one_pixel(min, max, viewport, viewport));
        camera.dolly_clamped(-10.0, 1.0, 0.1, top);
        assert_close(camera.observation_distance(), top);
    }
}
