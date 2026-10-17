//! Storage-neutral data and JSON persistence.
pub mod models;
pub mod persistence;
pub mod store;

#[cfg(test)]
pub mod fixtures;

#[cfg(test)]
mod durability_tests;

#[cfg(test)]
mod migrated_data_tests;
