//! LSP Converter's native core (forked from ACECM's): the UI's HTTP server
//! and the converter's routes, as a library so the app binary and the dev
//! tool in src/bin share one implementation. The crate keeps ACECM's name,
//! `acecm`, inside the code only.

pub mod api;
pub mod archive;
pub mod auth;
pub mod bundle;
pub mod config;
pub mod convert;
pub mod detect;
pub mod edition;
pub mod jsoncache;
pub mod logs;
pub mod mods;
pub mod paths;
pub mod procs;
pub mod server;
pub mod staging;
pub mod web;

// the game's message schemas, read from the user's own exe: the phys_json
// dev tool only, never part of the app (LEGAL-REVIEW.md)
#[cfg(feature = "dev-schemas")]
pub mod protos;
