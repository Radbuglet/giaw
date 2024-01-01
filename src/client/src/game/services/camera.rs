use giaw_shared::{
    game::services::transform::{EntityExt, Transform},
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

    pub fn project(&mut self, pos: Vec2) -> Vec2 {
        self.camera()
            .map_or(pos, |camera| camera.get().project(pos))
    }

    pub fn de_project(&mut self, pos: Vec2) -> Vec2 {
        self.camera()
            .map_or(pos, |camera| camera.get().de_project(pos))
    }
}

#[derive(Debug)]
pub struct VirtualCamera {
    transform: Obj<Transform>,
    aabb: Aabb,
    constraints: VirtualCameraConstraints,

    // Caches
    last_viewport_size: Vec2,
    screen_to_world_ogl: Affine2,
    world_to_screen_ogl: Affine2,
    screen_to_world_px: Affine2,
    world_to_screen_px: Affine2,
}

impl VirtualCamera {
    pub fn new(aabb: Aabb, constraints: VirtualCameraConstraints) -> impl CyclicCtor<Self> {
        move |me, _| Self {
            transform: me.obj(),
            aabb,
            constraints,
            last_viewport_size: Vec2::ONE,
            screen_to_world_ogl: Affine2::IDENTITY,
            world_to_screen_ogl: Affine2::IDENTITY,
            screen_to_world_px: Affine2::IDENTITY,
            world_to_screen_px: Affine2::IDENTITY,
        }
    }

    pub fn new_attached(
        aabb: Aabb,
        constraints: VirtualCameraConstraints,
    ) -> impl CyclicCtor<Self> {
        move |me, ob| {
            me.deep_obj::<CameraManager>().get_mut().push(ob.clone());
            Self::new(aabb, constraints)(me, ob)
        }
    }

    pub fn visible_aabb(&self) -> Aabb {
        let xform = self.transform.get().global_xform();

        let corners = self
            .aabb()
            .corners()
            .map(|corner| xform.transform_point2(corner));

        Aabb::new_poly(&corners)
    }

    pub fn aabb(&self) -> Aabb {
        self.aabb
    }

    pub fn set_aabb(&mut self, aabb: Aabb) {
        self.aabb = aabb;
    }

    pub fn update(&mut self, viewport_size: Vec2) {
        self.last_viewport_size = viewport_size;

        // Apply constraints
        if let Some(kept_area) = self.constraints.keep_area {
            let size = viewport_size;
            let size = size * (kept_area / (size.x * size.y)).sqrt();
            self.aabb = Aabb::new_centered(self.aabb.center(), size);
        }

        // Update the matrices
        {
            // We're trying to construct a matrix from OpenGL screen coordinates to world coordinates.
            let mat = Affine2::IDENTITY;

            // First, scale the OpenGL screen box into the local-space AABB.
            // Recall that matrix multiplication is right-associative in Glam. We want the matrices to
            // apply in the same order in which they apply in code, which means that we're always pushing
            // matrices to the left of the active one.

            // Scale... (N.B. we use a y-down system)
            let mat = Affine2::from_scale(self.aabb.size() * Vec2::new(1., -1.) / 2.) * mat;

            // ...then translate!
            let mat = Affine2::from_translation(self.aabb.center()) * mat;

            // Now that the camera is mapped to the AABB's bounds in local space, we can convert that
            // into world-space coordinates.
            let mat = self.transform.get().global_xform() * mat;

            // We now have a affine transformation from OpenGL coordinates to world coordinates and
            // its inverse.
            self.screen_to_world_ogl = mat;
            self.world_to_screen_ogl = mat.inverse();

            // Finally, let's derive a pixel-relative version of it.
            self.world_to_screen_px = Affine2::from_translation(viewport_size / 2.)
                * Affine2::from_scale(viewport_size * Vec2::new(0.5, -0.5))
                * self.world_to_screen_ogl;

            self.screen_to_world_px = self.world_to_screen_px.inverse();
        }
    }

    pub fn screen_to_world_ogl(&self) -> Affine2 {
        self.screen_to_world_ogl
    }

    pub fn world_to_screen_ogl(&self) -> Affine2 {
        self.world_to_screen_ogl
    }

    pub fn screen_to_world_px(&self) -> Affine2 {
        self.screen_to_world_px
    }

    pub fn world_to_screen_px(&self) -> Affine2 {
        self.world_to_screen_px
    }

    pub fn project(&self, pos: Vec2) -> Vec2 {
        self.screen_to_world_px().transform_point2(pos)
    }

    pub fn de_project(&self, pos: Vec2) -> Vec2 {
        self.world_to_screen_px().transform_point2(pos)
    }

    pub fn snapshot(&self) -> VirtualCameraSnapshot {
        let mat = self.world_to_screen_ogl;
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
