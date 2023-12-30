use glam::{BVec2, Vec2};

pub trait Vec2Ext {
    fn mask(self, mask: BVec2) -> Self;
    fn mask_in_axis(self, axis: Axis2) -> Self;
    fn mask_out_axis(self, axis: Axis2) -> Self;
    fn get_axis(self, axis: Axis2) -> f32;
}

impl Vec2Ext for Vec2 {
    fn mask(self, mask: BVec2) -> Self {
        Self::select(mask, self, Vec2::ZERO)
    }

    fn mask_in_axis(self, axis: Axis2) -> Self {
        self.mask(axis.mask())
    }

    fn mask_out_axis(self, axis: Axis2) -> Self {
        self.mask(!axis.mask())
    }

    fn get_axis(self, axis: Axis2) -> f32 {
        self[axis as usize]
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub enum Axis2 {
    X,
    Y,
}

impl Axis2 {
    pub const AXES: [Self; 2] = [Self::X, Self::Y];

    pub fn iter() -> impl Iterator<Item = Self> {
        Self::AXES.into_iter()
    }

    pub fn mask(self) -> BVec2 {
        match self {
            Axis2::X => BVec2::new(true, false),
            Axis2::Y => BVec2::new(false, true),
        }
    }

    pub fn unit_mag(self, comp: f32) -> Vec2 {
        match self {
            Axis2::X => Vec2::new(comp, 0.),
            Axis2::Y => Vec2::new(0., comp),
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub enum Sign {
    Positive,
    Negative,
}

impl Sign {
    pub fn of_biased(v: f32) -> Self {
        if v < 0. {
            Self::Negative
        } else {
            Self::Positive
        }
    }

    pub fn unit_mag(self, v: f32) -> f32 {
        if self == Sign::Negative {
            -v
        } else {
            v
        }
    }
}

pub fn add_magnitude(v: f32, by: f32) -> f32 {
    v + Sign::of_biased(v).unit_mag(by)
}
