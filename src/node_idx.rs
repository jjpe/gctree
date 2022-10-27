//!

#[rustfmt::skip]
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde_derive::Deserialize,
    serde_derive::Serialize,
    derive_more::From
)]
pub struct NodeIdx(pub(crate) usize);

impl NodeIdx {
    pub const TREE_ROOT: Self = Self(0);
}

impl std::fmt::Debug for NodeIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "NodeIdx({})", self.0)
    }
}

impl std::fmt::Display for NodeIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Add<usize> for NodeIdx {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: usize) -> Self {
        Self(self.0 + rhs)
    }
}

impl std::ops::Sub<usize> for NodeIdx {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: usize) -> Self {
        Self(self.0 - rhs)
    }
}
