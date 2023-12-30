use std::iter;

use glam::{IVec2, Vec2};

#[derive(Debug, Copy, Clone)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    pub const NAN: Self = Self {
        min: Vec2::NAN,
        max: Vec2::NAN,
    };

    pub const ZERO: Self = Self {
        min: Vec2::ZERO,
        max: Vec2::ZERO,
    };

    pub const ZERO_TO_ONE: Self = Self {
        min: Vec2::ZERO,
        max: Vec2::ONE,
    };

    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self::new_sized(Vec2::new(x, y), Vec2::new(w, h))
    }

    pub fn new_centered(center: Vec2, size: Vec2) -> Self {
        let half_size = size / 2.0;
        Self {
            min: center - half_size,
            max: center + half_size,
        }
    }

    pub fn new_sized(min: Vec2, size: Vec2) -> Self {
        Self {
            min,
            max: min + size,
        }
    }

    pub fn translated(self, rel: Vec2) -> Self {
        Self {
            min: self.min + rel,
            max: self.max + rel,
        }
    }

    pub fn translate_extend(self, rel: Vec2) -> Self {
        let target = self.translated(rel);
        Self {
            min: self.min.min(target.min),
            max: self.max.max(target.max),
        }
    }

    pub fn contains(self, point: Vec2) -> bool {
        (self.min.cmple(point) & point.cmple(self.max)).all()
    }

    pub fn intersects(self, other: Self) -> bool {
        !(
            // We're entirely to the left
            self.max.x < other.min.x ||
			// We're entirely to the right
			other.max.x < self.min.x ||
			// We're entirely above
			self.max.y < other.min.y ||
			// We're entirely below
			other.max.y < self.min.y
        )
    }

    pub fn normalized(self) -> Self {
        let min = self.min.min(self.max);
        let max = self.min.max(self.max);
        Self { min, max }
    }

    pub fn clamped(self) -> Self {
        Self {
            min: self.min.min(self.max),
            max: self.max.max(self.min),
        }
    }

    pub fn clamp(self, pos: Vec2) -> Vec2 {
        pos.clamp(self.min, self.max)
    }

    pub fn grow(self, by: Vec2) -> Self {
        Self::new_centered(self.center(), self.size() + by)
    }

    pub fn shrink(self, by: Vec2) -> Self {
        self.grow(-by)
    }

    pub fn center(self) -> Vec2 {
        self.min.lerp(self.max, 0.5)
    }

    pub fn size(self) -> Vec2 {
        self.max - self.min
    }

    pub fn x(self) -> f32 {
        self.min.x
    }

    pub fn y(self) -> f32 {
        self.min.y
    }

    pub fn w(self) -> f32 {
        self.size().x
    }

    pub fn h(self) -> f32 {
        self.size().y
    }

    pub fn is_nan(self) -> bool {
        self.min.is_nan() || self.max.is_nan()
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct AabbI {
    pub min: IVec2,
    pub max: IVec2,
}

impl AabbI {
    pub const ZERO: AabbI = AabbI {
        min: IVec2::ZERO,
        max: IVec2::ZERO,
    };

    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self::new_sized(IVec2::new(x, y), IVec2::new(w, h))
    }

    pub const fn new_sized(min: IVec2, size: IVec2) -> Self {
        Self {
            min,
            max: IVec2::new(min.x + size.x, min.y + size.y),
        }
    }

    pub fn normalized(self) -> Self {
        let min = self.min.min(self.max);
        let max = self.min.max(self.max);
        Self { min, max }
    }

    pub fn inclusive(self) -> Self {
        Self {
            min: self.min,
            max: self.max + IVec2::ONE,
        }
    }

    pub fn iter(mut self) -> impl Iterator<Item = IVec2> {
        self = self.normalized();

        let mut pos = self.min - IVec2::X;
        iter::from_fn(move || {
            if pos.x < self.max.x {
                pos.x += 1;
            } else {
                pos.x = self.min.x;
                pos.y += 1;
            }

            (pos.y < self.max.y).then_some(pos)
        })
    }

    pub fn size(&self) -> IVec2 {
        self.max - self.min
    }

    pub fn as_aabb(self) -> Aabb {
        Aabb {
            min: self.min.as_vec2(),
            max: self.max.as_vec2(),
        }
    }
}