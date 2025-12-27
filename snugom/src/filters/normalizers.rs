//! Common normalizer functions for filter values
//!
//! This module contains only truly generic utilities that are useful across multiple services.
//! Service-specific normalizers should be defined within their respective service modules.

use crate::errors::RepoError;
use crate::search::{FilterCondition, FilterDescriptor, FilterOperator};

/// Builds a numeric filter for range queries
pub fn build_numeric_filter(descriptor: FilterDescriptor, target_field: &str) -> Result<FilterCondition, RepoError> {
    match descriptor.operator {
        FilterOperator::Eq => {
            let value = descriptor.values.first().ok_or_else(|| RepoError::InvalidRequest {
                message: format!("Numeric filter on {} requires a value", target_field),
            })?;
            let numeric = value.parse::<f64>().map_err(|_| RepoError::InvalidRequest {
                message: format!("Invalid numeric value: {}", value),
            })?;
            Ok(FilterCondition::NumericRange {
                field: target_field.to_string(),
                min: Some(numeric),
                max: Some(numeric),
            })
        }
        FilterOperator::Range => {
            let min = parse_numeric_bound(descriptor.values.first())?;
            let max = parse_numeric_bound(descriptor.values.get(1))?;
            Ok(FilterCondition::NumericRange {
                field: target_field.to_string(),
                min,
                max,
            })
        }
        FilterOperator::Bool => Err(RepoError::InvalidRequest {
            message: format!("Boolean operator is not supported for numeric field {}", target_field),
        }),
        FilterOperator::Prefix | FilterOperator::Contains | FilterOperator::Exact | FilterOperator::Fuzzy => {
            Err(RepoError::InvalidRequest {
                message: format!(
                    "Text operators (prefix, contains, exact, fuzzy) are not supported for numeric field {}",
                    target_field
                ),
            })
        }
    }
}

/// Parses a numeric bound value, handling special cases like "*" for unbounded
pub fn parse_numeric_bound(value: Option<&String>) -> Result<Option<f64>, RepoError> {
    match value {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed == "*" {
                Ok(None)
            } else {
                trimmed.parse::<f64>().map(Some).map_err(|_| RepoError::InvalidRequest {
                    message: format!("Invalid numeric bound: {}", trimmed),
                })
            }
        }
        None => Ok(None),
    }
}

/// Builds a TEXT prefix filter for prefix matching on TEXT fields
pub fn build_text_prefix_filter(descriptor: FilterDescriptor, target_field: &str) -> Result<FilterCondition, RepoError> {
    if descriptor.operator != FilterOperator::Prefix {
        return Err(RepoError::InvalidRequest {
            message: format!("Expected prefix operator for TEXT field {}", target_field),
        });
    }
    let value = descriptor.values.into_iter().next().ok_or_else(|| RepoError::InvalidRequest {
        message: format!("Prefix filter on {} requires a value", target_field),
    })?;
    Ok(FilterCondition::TextPrefix {
        field: target_field.to_string(),
        value,
    })
}

/// Builds a TEXT contains filter for substring matching on TEXT fields
pub fn build_text_contains_filter(descriptor: FilterDescriptor, target_field: &str) -> Result<FilterCondition, RepoError> {
    if descriptor.operator != FilterOperator::Contains {
        return Err(RepoError::InvalidRequest {
            message: format!("Expected contains operator for TEXT field {}", target_field),
        });
    }
    let value = descriptor.values.into_iter().next().ok_or_else(|| RepoError::InvalidRequest {
        message: format!("Contains filter on {} requires a value", target_field),
    })?;
    Ok(FilterCondition::TextContains {
        field: target_field.to_string(),
        value,
    })
}

/// Builds a TEXT exact filter for exact phrase matching on TEXT fields
pub fn build_text_exact_filter(descriptor: FilterDescriptor, target_field: &str) -> Result<FilterCondition, RepoError> {
    if descriptor.operator != FilterOperator::Exact {
        return Err(RepoError::InvalidRequest {
            message: format!("Expected exact operator for TEXT field {}", target_field),
        });
    }
    let value = descriptor.values.into_iter().next().ok_or_else(|| RepoError::InvalidRequest {
        message: format!("Exact filter on {} requires a value", target_field),
    })?;
    Ok(FilterCondition::TextExact {
        field: target_field.to_string(),
        value,
    })
}

/// Builds a TEXT fuzzy filter for fuzzy matching on TEXT fields
pub fn build_text_fuzzy_filter(descriptor: FilterDescriptor, target_field: &str) -> Result<FilterCondition, RepoError> {
    if descriptor.operator != FilterOperator::Fuzzy {
        return Err(RepoError::InvalidRequest {
            message: format!("Expected fuzzy operator for TEXT field {}", target_field),
        });
    }
    let value = descriptor.values.into_iter().next().ok_or_else(|| RepoError::InvalidRequest {
        message: format!("Fuzzy filter on {} requires a value", target_field),
    })?;
    Ok(FilterCondition::TextFuzzy {
        field: target_field.to_string(),
        value,
    })
}

/// Builds the appropriate TEXT filter based on the operator
pub fn build_text_filter(descriptor: FilterDescriptor, target_field: &str) -> Result<FilterCondition, RepoError> {
    match descriptor.operator {
        FilterOperator::Prefix => build_text_prefix_filter(descriptor, target_field),
        FilterOperator::Contains => build_text_contains_filter(descriptor, target_field),
        FilterOperator::Exact => build_text_exact_filter(descriptor, target_field),
        FilterOperator::Fuzzy => build_text_fuzzy_filter(descriptor, target_field),
        FilterOperator::Eq => {
            // For backwards compatibility, Eq on TEXT fields creates a prefix filter
            let value = descriptor.values.into_iter().next().ok_or_else(|| RepoError::InvalidRequest {
                message: format!("Filter on {} requires a value", target_field),
            })?;
            Ok(FilterCondition::TextPrefix {
                field: target_field.to_string(),
                value,
            })
        }
        other => Err(RepoError::InvalidRequest {
            message: format!("Operator {:?} is not supported for TEXT field {}", other, target_field),
        }),
    }
}
