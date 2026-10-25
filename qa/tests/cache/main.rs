//! Tests that check a cache of the whole process, the highlight cache of
//! tiza-core. They are in a crate of their own, so that no other test evicts
//! an entry while they look.

mod one_layout_per_frame;
