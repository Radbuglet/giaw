use giaw_shared::{
    game::services::transform::Transform,
    util::{
        lang::{entity::CyclicCtor, obj::Obj},
        math::aabb::Aabb,
    },
};
use macroquad::{
    camera::Camera,
    math::{Affine2, Mat4, Vec2, Vec4},
    miniquad::RenderPass,
};

// === Core === //

#[derive(Debug, Default)]
pub struct CameraManager {
    stack: Vec<Obj<VirtualCamera>>,
}

impl CameraManager {
    pub fn push(&mut self, camera: Obj<VirtualCamera>) {
        self.stack.push(camera);
    }

    pub fn remove(&mut self, camera: &Obj<VirtualCamera>) {
        self.stack.retain(|v| v == camera)
    }

    pub fn camera(&mut self) -> Option<&Obj<VirtualCamera>> {
        self.stack.retain(Obj::is_alive);
        self.stack.last()
    }

    pub fn camera_snapshot(&mut self, viewport_size: Vec2) -> Option<VirtualCameraSnapshot> {
        self.camera().map(|c| {
            let mut c = c.get_mut();
            c.constrain(viewport_size);
            c.snapshot()
        })
    }
}

#[derive(Debug)]
pub struct VirtualCamera {
    transform: Obj<Transform>,
    aabb: Aabb,
    constraints: VirtualCameraConstraints,
}

impl VirtualCamera {
    pub fn new(aabb: Aabb, constraints: VirtualCameraConstraints) -> impl CyclicCtor<Self> {
        move |me, _| Self {
            transform: me.obj(),
            aabb,
            constraints,
        }
    }

    pub fn new_constrained(constraints: VirtualCameraConstraints) -> impl CyclicCtor<Self> {
        Self::new(Aabb::ZERO, constraints)
    }

    pub fn aabb(&self) -> Aabb {
        self.aabb
    }

    pub fn set_aabb(&mut self, aabb: Aabb) {
        self.aabb = aabb;
    }

    pub fn constrain(&mut self, viewport_size: Vec2) {
        if let Some(kept_area) = self.constraints.keep_area {
            let size = viewport_size;
            let size = size * (kept_area / (size.x * size.y)).sqrt();
            self.aabb = Aabb::new_centered(self.aabb.center(), size);
        }
    }

    pub fn snapshot(&self) -> VirtualCameraSnapshot {
        // We're trying to construct a matrix from OpenGL screen coordinates to world coordinates.
        let mat = Affine2::IDENTITY;

        // First, scale the OpenGL screen box into the local-space AABB.
        // Recall that matrix multiplication is right-associative in Glam. We want the matrices to
        // apply in the same order in which they apply in code, which means that we're always pushing
        // matrices to the left of the active one.

        let mat = Affine2::from_scale(self.aabb.size()) * mat; // Scale...
        let mat = Affine2::from_translation(self.aabb.center()) * mat; // ...then translate!

        // Now that the camera is mapped to the AABB's bounds in local space, we can convert that
        // into world-space coordinates.
        let mat = self.transform.get().global_xform() * mat;

        // We now have a affine transformation from OpenGL coordinates to world coordinates. We need
        // to invert that.
        let mat = mat.inverse();

        // Finally, we need to extend this 2D affine transformation into a 3D one.
        let mat = Mat4::from_cols(
            mat.x_axis.extend(0.).extend(0.),
            mat.y_axis.extend(0.).extend(0.),
            Vec4::new(0., 0., 1., 0.),
            mat.translation.extend(0.).extend(1.),
        );

        VirtualCameraSnapshot(mat)
    }
}

#[derive(Debug, Clone)]
pub struct VirtualCameraSnapshot(Mat4);

impl Camera for VirtualCameraSnapshot {
    fn matrix(&self) -> Mat4 {
        self.0
    }

    fn depth_enabled(&self) -> bool {
        true
    }

    fn render_pass(&self) -> Option<RenderPass> {
        None
    }

    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        None
    }
}

#[derive(Debug, Clone, Default)]
pub struct VirtualCameraConstraints {
    pub keep_area: Option<f32>,
}

impl VirtualCameraConstraints {
    pub fn keep_visible_area(mut self, area: Vec2) -> Self {
        self.keep_area = Some(area.x * area.y);
        self
    }
}
