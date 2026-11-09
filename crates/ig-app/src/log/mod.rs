//! The read-only Git Log: background loading ([`worker`]), the UI-side state ([`model`]) and the
//! table ([`view`]) with its ported painters ([`graph_painter`], [`labels`]) and helpers.

pub mod date;
pub mod graph_painter;
pub mod labels;
pub mod metadata;
pub mod model;
pub mod ref_groups;
pub mod selection;
pub mod view;
pub mod worker;
