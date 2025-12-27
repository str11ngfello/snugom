//! Shared filter normalizers and utilities for searchable fields
//!
//! This module contains reusable normalizer functions that can be used
//! across different services when implementing searchable filters.

pub mod normalizers;

pub use normalizers::*;
