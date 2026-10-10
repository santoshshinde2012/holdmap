//! Validate the small JSON Schema vocabulary used by the published tool contracts.
//!
//! Input validation uses discovery metadata directly, before any scan, probe or stop.

use serde_json::Value;

pub(super) fn validate(value: &Value, schema: &Value) -> Result<(), String> {
    at(value, schema, "arguments")
}

fn at(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    if let Some(alternatives) = schema["anyOf"].as_array() {
        if !alternatives
            .iter()
            .any(|schema| at(value, schema, path).is_ok())
        {
            return Err(format!(
                "`{path}` does not match any supported result shape"
            ));
        }
    }
    let matches_type = |kind: &str| match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        // JSON Schema permits 3000.0 as an integer, but never rounds fractional ports.
        "integer" => value.as_f64().is_some_and(|number| number.fract() == 0.0),
        "number" => value.is_number(),
        "null" => value.is_null(),
        _ => false,
    };
    if let Some(kind) = schema["type"].as_str() {
        if !matches_type(kind) {
            return Err(format!("`{path}` must be {kind}"));
        }
    } else if let Some(kinds) = schema["type"].as_array() {
        if !kinds.iter().filter_map(Value::as_str).any(matches_type) {
            return Err(format!("`{path}` has an unsupported value type"));
        }
    }
    if let Some(allowed) = schema["enum"].as_array() {
        if !allowed.contains(value) {
            let choices = allowed.iter().map(Value::to_string).collect::<Vec<_>>();
            return Err(format!("`{path}` must be one of {}", choices.join(", ")));
        }
    }
    if let Some(number) = value.as_f64() {
        for (keyword, comparison) in [("minimum", true), ("maximum", false)] {
            if let Some(bound) = schema[keyword].as_f64() {
                if (comparison && number < bound) || (!comparison && number > bound) {
                    return Err(format!(
                        "`{path}` must be {} {bound}",
                        if comparison { "at least" } else { "at most" }
                    ));
                }
            }
        }
    }
    if let Some(text) = value.as_str() {
        if let Some(maximum) = schema["maxLength"].as_u64() {
            if (text.chars().count() as u64) > maximum {
                return Err(format!(
                    "`{path}` must contain at most {maximum} character(s)"
                ));
            }
        }
        if let Some(minimum) = schema["minLength"].as_u64() {
            if (text.chars().count() as u64) < minimum {
                return Err(format!(
                    "`{path}` must contain at least {minimum} character(s)"
                ));
            }
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema["required"].as_array() {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("`{path}.{key}` is required"));
                }
            }
        }
        for (key, value) in object {
            if let Some(property) = schema["properties"].get(key) {
                at(value, property, &format!("{path}.{key}"))?;
            } else if schema["additionalProperties"] == false {
                let allowed = schema["properties"]
                    .as_object()
                    .map(|properties| properties.keys().cloned().collect::<Vec<_>>().join(", "))
                    .unwrap_or_default();
                return Err(format!(
                    "Unknown argument `{key}`; accepted arguments: {allowed}"
                ));
            }
        }
    }
    if let (Some(values), Some(items)) = (value.as_array(), schema.get("items")) {
        for (index, value) in values.iter().enumerate() {
            at(value, items, &format!("{path}[{index}]"))?;
        }
    }
    if let Some(condition) = schema.get("if") {
        let branch = if at(value, condition, path).is_ok() {
            "then"
        } else {
            "else"
        };
        if let Some(schema) = schema.get(branch) {
            at(value, schema, path)?;
        }
    }
    Ok(())
}
