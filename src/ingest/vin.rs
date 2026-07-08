use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VinMetadata {
    pub make: String,
    pub model: String,
    pub engine_family: String,
    pub year: i32,
    pub powertrain: String,
    pub displacement_l: String,
    pub cylinders: String,
    pub engine_configuration: String,
    pub aspiration: String,
    pub body_class: String,
    pub trim: String,
}

pub fn decode(vin: &str) -> VinMetadata {
    let vin = vin.trim().to_ascii_uppercase();
    if !is_valid_vin(&vin) {
        return VinMetadata::default();
    }
    VinMetadata {
        make: make(&vin).to_string(),
        year: year(&vin),
        ..Default::default()
    }
}

pub fn is_valid_vin(vin: &str) -> bool {
    let vin = vin.trim().to_ascii_uppercase();
    vin.len() == 17
        && vin.chars().all(|ch| ch.is_ascii_alphanumeric())
        && !vin.chars().any(|ch| matches!(ch, 'I' | 'O' | 'Q'))
}

pub fn pattern_key(vin: &str, year: i32) -> Option<(String, i32)> {
    let vin = vin.trim().to_ascii_uppercase();
    if !is_valid_vin(&vin) || year <= 0 {
        return None;
    }
    Some((vin.chars().take(8).collect(), year))
}

pub fn map_vpic_result(vin: &str, requested_year: i32, row: &Value) -> VinMetadata {
    let model = pick(row, "Model");
    let mut metadata = VinMetadata {
        year: parse_year(&pick(row, "ModelYear")).unwrap_or(requested_year.max(0)),
        make: pick(row, "Make"),
        model: if model.is_empty() {
            pick(row, "ModelName")
        } else {
            model
        },
        powertrain: normalize_powertrain(row),
        displacement_l: format_displacement(&pick(row, "DisplacementL")),
        cylinders: pick(row, "EngineCylinders"),
        engine_configuration: pick(row, "EngineConfiguration"),
        aspiration: normalize_aspiration(row),
        body_class: pick(row, "BodyClass"),
        trim: pick(row, "Trim"),
        ..Default::default()
    };
    if metadata.year <= 0 {
        metadata.year = decode(vin).year;
    }
    metadata.engine_family = normalize_engine_family(&metadata);
    metadata
}

pub fn lookup_status(metadata: &VinMetadata) -> &'static str {
    if !metadata.make.is_empty() && !metadata.model.is_empty() && !metadata.engine_family.is_empty()
    {
        "ok"
    } else {
        "incomplete"
    }
}

fn year(vin: &str) -> i32 {
    let Some(code) = vin.chars().nth(9) else {
        return 0;
    };
    let Some(offset) = "ABCDEFGHJKLMNPRSTVWXY123456789"
        .chars()
        .position(|candidate| candidate == code)
    else {
        return 0;
    };
    let base = 1980 + offset as i32;
    let cycle_hint = vin.chars().nth(6).unwrap_or('0');
    if base <= 2009 && cycle_hint.is_ascii_alphabetic() {
        base + 30
    } else {
        base
    }
}

fn make(vin: &str) -> &'static str {
    let wmi = vin.get(0..3).unwrap_or("");
    match wmi {
        "1FA" | "1FB" | "1FC" | "1FD" | "1FM" | "1FT" | "2FM" | "3FA" => "Ford",
        "1G1" | "1G6" | "2G1" | "3G1" => "Chevrolet",
        "1GC" | "2GC" | "3GC" => "Chevrolet",
        "1G4" | "2G4" | "3G4" => "Buick",
        "1HG" | "2HG" | "3HG" | "5FN" | "JHM" => "Honda",
        "1C3" | "1C4" | "1C6" | "2C3" | "2C4" | "2C6" | "3C3" | "3C4" | "3C6" => "Chrysler",
        "1N4" | "3N1" | "5N1" | "JN1" | "JN8" => "Nissan",
        "2T1" | "4T1" | "4T3" | "5TD" | "JTD" | "JT3" | "JT4" => "Toyota",
        "KMH" | "5NP" => "Hyundai",
        "KNA" | "KND" | "5XY" => "Kia",
        "WBA" | "WBS" | "WBY" | "5UX" => "BMW",
        "WDD" | "WDC" | "4JG" => "Mercedes-Benz",
        "WAU" | "WA1" | "TRU" => "Audi",
        "WVW" | "3VW" => "Volkswagen",
        "YV1" | "YV4" => "Volvo",
        _ => "",
    }
}

fn pick(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn parse_year(value: &str) -> Option<i32> {
    let year = value.trim().parse::<i32>().ok()?;
    (year > 0).then_some(year)
}

fn normalize_powertrain(row: &Value) -> String {
    let fields = [
        pick(row, "ElectrificationLevel"),
        pick(row, "EngineConfiguration"),
        pick(row, "FuelTypePrimary"),
        pick(row, "FuelTypeSecondary"),
    ]
    .join(" ")
    .to_ascii_lowercase();
    if fields.contains("plug-in hybrid") || fields.contains("phev") {
        "PHEV".into()
    } else if fields.contains("hybrid") || fields.contains("hev") {
        "Hybrid".into()
    } else if fields.contains("electric") || fields.contains("bev") || fields.trim() == "ev" {
        "EV".into()
    } else {
        String::new()
    }
}

fn normalize_aspiration(row: &Value) -> String {
    let aspiration = pick(row, "AspirationType");
    let value = if aspiration.is_empty() {
        pick(row, "Turbo")
    } else {
        aspiration
    };
    match value.to_ascii_lowercase().as_str() {
        "yes" | "true" | "1" => "turbo".into(),
        "no" | "false" | "0" => String::new(),
        _ => value,
    }
}

fn format_displacement(value: &str) -> String {
    let text = value.trim();
    if text.is_empty() {
        return String::new();
    }
    text.parse::<f64>()
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|_| text.to_string())
}

fn normalize_cylinder_layout(value: &str) -> String {
    let text = value.trim().to_ascii_lowercase();
    if text.is_empty() {
        return String::new();
    }
    if text.contains("v-shaped") || text == "v" {
        "V".into()
    } else if text.contains("inline") || matches!(text.as_str(), "i" | "in-line") {
        "I".into()
    } else if text.contains("flat") || text.contains("boxer") || text == "h" {
        "H".into()
    } else if text.contains('w') && (text.contains("shaped") || text == "w") {
        "W".into()
    } else {
        String::new()
    }
}

fn normalize_engine_family(metadata: &VinMetadata) -> String {
    match metadata.powertrain.to_ascii_lowercase().as_str() {
        "ev" => return "EV".into(),
        "phev" => return "PHEV".into(),
        "hybrid" => return "Hybrid".into(),
        _ => {}
    }

    if metadata.displacement_l.is_empty() || metadata.cylinders.is_empty() {
        return String::new();
    }
    let aspiration = match metadata.aspiration.to_ascii_lowercase().as_str() {
        "" | "naturally aspirated" | "na" | "no" | "false" | "0" => "NA".into(),
        "turbo" | "turbocharged" | "yes" | "true" | "1" => "Turbo".into(),
        _ => metadata.aspiration.clone(),
    };
    let layout = normalize_cylinder_layout(&metadata.engine_configuration);
    let cylinders = if layout.is_empty() {
        format!("{}cyl", metadata.cylinders)
    } else {
        format!("{layout}{}", metadata.cylinders)
    };
    format!("{}L {cylinders} {aspiration}", metadata.displacement_l)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SAMPLE_VIN: &str = "TSTHNDAXXRA000001";

    #[test]
    fn decodes_common_make_and_year() {
        let metadata = decode(SAMPLE_VIN);
        assert_eq!(make("1HG"), "Honda");
        assert_eq!(metadata.year, 2024);
        assert_eq!(metadata.model, "");
    }

    #[test]
    fn maps_repeated_year_codes_to_recent_cycle() {
        assert_eq!(decode("TSTHNDAXXNA000001").year, 2022);
    }

    #[test]
    fn unknown_or_short_vins_return_blanks() {
        let metadata = decode("DEMO-HONDA-ACCORD");
        assert_eq!(metadata.make, "");
        assert_eq!(metadata.model, "");
        assert_eq!(metadata.year, 0);
    }

    #[test]
    fn validates_vin_shape() {
        assert!(is_valid_vin(SAMPLE_VIN));
        assert!(!is_valid_vin("TSTHNDAXXRA00000I"));
        assert!(!is_valid_vin("short"));
    }

    #[test]
    fn builds_exact_pattern_key_only_for_valid_vins() {
        assert_eq!(
            pattern_key(SAMPLE_VIN, 2011),
            Some(("TSTHNDAX".into(), 2011))
        );
        assert_eq!(pattern_key("DEMO-HONDA-ACCORD", 2011), None);
    }

    #[test]
    fn maps_vpic_result_without_guessing_cylinder_layout() {
        let metadata = map_vpic_result(
            SAMPLE_VIN,
            2011,
            &json!({
                "ModelYear": "2011",
                "Make": "Honda",
                "Model": "Accord",
                "DisplacementL": "3.456",
                "EngineCylinders": "6",
                "Turbo": "No"
            }),
        );
        assert_eq!(metadata.engine_family, "3.5L 6cyl NA");
        assert_eq!(lookup_status(&metadata), "ok");
    }

    #[test]
    fn maps_vpic_result_with_explicit_layout() {
        let metadata = map_vpic_result(
            SAMPLE_VIN,
            2011,
            &json!({
                "ModelYear": "2011",
                "Make": "Honda",
                "Model": "Accord",
                "DisplacementL": "3.456",
                "EngineCylinders": "6",
                "EngineConfiguration": "V-Shaped",
                "Turbo": "No"
            }),
        );
        assert_eq!(metadata.engine_family, "3.5L V6 NA");
    }
}
