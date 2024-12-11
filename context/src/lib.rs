// NOTE: This lint isn't applicable since we compile with nightly
/* trunk-ignore(clippy/E0554) */
#![feature(round_char_boundary)]

pub mod bazel_bep;
pub mod env;
pub mod junit;
pub mod meta;
pub mod repo;
mod string_safety;
