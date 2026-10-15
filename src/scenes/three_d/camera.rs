use bevy::{
    math::{Vec3, proj::perspective},
    prelude::Transform,
};

use super::{model::InitialConditions, render::floats};

pub struct Camera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 14.0,
            yaw: -1.1,
            pitch: 0.7,
        }
    }
}

impl Camera {
    pub fn direction(&self) -> Vec3 {
        Vec3::new(
            self.pitch.cos() * self.yaw.cos(),
            self.pitch.cos() * self.yaw.sin(),
            self.pitch.sin(),
        )
    }

    pub fn fit(&mut self, state: &InitialConditions, aspect: f32) {
        let positions = state
            .bodies
            .map(|body| Vec3::from_array(body.position.map(|v| v as f32)));
        let low = positions
            .into_iter()
            .fold(Vec3::splat(f32::INFINITY), Vec3::min);
        let high = positions
            .into_iter()
            .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
        self.target = (low + high) * 0.5;
        let radius = positions
            .into_iter()
            .map(|p| p.distance(self.target))
            .fold(0.1, f32::max);
        let half_fov = (0.5_f32 * std::f32::consts::FRAC_PI_4).tan();
        let angle = (half_fov * aspect.clamp(0.01, 1.0)).atan();
        self.distance = (radius / angle.sin() * 1.3).clamp(0.001, 1e14);
    }

    pub fn orbit(&mut self, delta: [f32; 2]) {
        self.yaw = (self.yaw - delta[0] * 0.008).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + delta[1] * 0.008).clamp(-1.5, 1.5);
    }

    pub fn pan(&mut self, delta: [f32; 2], height: f32) {
        let direction = self.direction();
        let right = Vec3::Z.cross(direction).normalize();
        let up = direction.cross(right);
        self.target += (-right * delta[0] + up * delta[1]) * self.distance * 0.8 / height.max(1.0);
    }

    pub fn uniform(&self, viewport: [f32; 2], opacity: f32, body_size: f32) -> Vec<u8> {
        let eye = self.target + self.direction() * self.distance;
        let view = Transform::from_translation(eye)
            .looking_at(self.target, Vec3::Z)
            .to_matrix()
            .inverse();
        let projection = perspective(
            std::f32::consts::FRAC_PI_4,
            viewport[0].max(1.0) / viewport[1].max(1.0),
            self.distance * 0.001,
            self.distance * 1000.0,
        );
        let mut bytes = Vec::with_capacity(128);
        floats(&mut bytes, (projection * view).to_cols_array());
        floats(&mut bytes, [eye.x, eye.y, eye.z, self.distance * 0.009]);
        let spacing = 10.0_f32.powf((self.distance * 0.08).log10().ceil());
        floats(
            &mut bytes,
            [self.target.x, self.target.y, self.target.z, spacing],
        );
        floats(&mut bytes, [viewport[0], viewport[1], opacity, body_size]);
        bytes.extend_from_slice(&[0; 16]);
        bytes
    }
}
