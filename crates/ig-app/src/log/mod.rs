//! The read-only Git Log: background loading ([`worker`]), the UI-side state ([`model`]) and the
//! ported painters ([`graph_painter`], [`labels`]) with their helpers. The table lands in the
//! next task.

pub mod date;
pub mod graph_painter;
pub mod labels;
pub mod metadata;
pub mod model;
pub mod ref_groups;
pub mod selection;
pub mod worker;
