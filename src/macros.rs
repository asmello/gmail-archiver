macro_rules! impl_display {
    ($($ty:ty),+) => {
        $(
            impl core::fmt::Display for $ty {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    self.0.fmt(f)
                }
            }
        )+
    };
}
pub(crate) use impl_display;

macro_rules! impl_as_str {
    ($($ty:ty),+) => {
        $(
            impl $ty {
                pub fn as_str(&self) -> &str {
                    &self.0
                }
            }
        )+
    };
}
pub(crate) use impl_as_str;

macro_rules! impl_from_string {
    ($($ty:ty),+) => {
        $(
            impl From<String> for $ty {
                fn from(value: String) -> Self {
                    Self(value.into())
                }
            }
        )+
    };
}
pub(crate) use impl_from_string;
