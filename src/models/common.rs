use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone)]
pub struct LosslessNumber(pub String);

impl fmt::Display for LosslessNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_for_json())
    }
}

impl Serialize for LosslessNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Format the number and serialize as a raw number (without quotes)
        let formatted = self.format_for_json();
        serializer.serialize_str(&formatted)
    }
}

impl<'de> Deserialize<'de> for LosslessNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Accept either string or number, convert to string
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(s) => Ok(LosslessNumber(s)),
            serde_json::Value::Number(n) => Ok(LosslessNumber(n.to_string())),
            _ => Err(serde::de::Error::custom("Expected string or number")),
        }
    }
}

impl LosslessNumber {
    pub fn new<T: ToString>(value: T) -> Self {
        LosslessNumber(value.to_string())
    }

    /// Formats the number for JSON serialization according to Plexo requirements
    /// - If it's an integer (no decimal point), append ".0"
    /// - If it's already a decimal, ensure it has exactly 2 decimal places
    /// - If it has more than 2 decimal places, truncate to 2
    pub fn format_for_json(&self) -> String {
        let trimmed = self.0.trim();

        // Try to parse as a number to validate and format
        if let Ok(num) = trimmed.parse::<f64>() {
            // Check if it's effectively an integer
            if num.fract() == 0.0 {
                format!("{:.1}", num) // This will format as "131.0"
            } else {
                format!("{:.2}", num) // This will format with 2 decimal places
            }
        } else {
            // If it can't be parsed as a number, return as-is but try to format it
            if trimmed.contains('.') {
                // It has a decimal point, try to ensure 2 decimal places
                let parts: Vec<&str> = trimmed.split('.').collect();
                if parts.len() == 2 {
                    let integer_part = parts[0];
                    let decimal_part = parts[1];

                    if decimal_part.len() == 1 {
                        format!("{}.{}0", integer_part, decimal_part)
                    } else if decimal_part.len() > 2 {
                        format!("{}.{}", integer_part, &decimal_part[..2])
                    } else {
                        trimmed.to_string()
                    }
                } else {
                    trimmed.to_string()
                }
            } else {
                // No decimal point, append ".0"
                format!("{}.0", trimmed)
            }
        }
    }
}
