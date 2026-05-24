//! JSON pass-through used by integrations. Currently consumers serialize their
//! own types via serde directly — this module is a thin alias for clarity.

use serde::Serialize;

pub fn pretty<T: Serialize>(value: &T) -> serde_json::Result<String> {
    serde_json::to_string_pretty(value)
}
