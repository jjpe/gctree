//!

#[rustfmt::skip]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde_derive::Deserialize,
    serde_derive::Serialize,
    derive_more::Deref,
    derive_more::From,
)]
pub struct NodeCount(usize);

impl std::ops::Add<Self> for NodeCount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::Sub<Self> for NodeCount {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}
