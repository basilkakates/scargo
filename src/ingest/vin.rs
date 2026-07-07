#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VinMetadata {
    pub make: &'static str,
    pub model: &'static str,
    pub year: i32,
}

pub fn decode(vin: &str) -> VinMetadata {
    let vin = vin.trim().to_ascii_uppercase();
    if vin.len() != 17 || vin.chars().any(|ch| matches!(ch, 'I' | 'O' | 'Q')) {
        return VinMetadata {
            make: "",
            model: "",
            year: 0,
        };
    }
    VinMetadata {
        make: make(&vin),
        model: "",
        year: year(&vin),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_common_make_and_year() {
        let metadata = decode("1HGXXXXXXRA000001");
        assert_eq!(metadata.make, "Honda");
        assert_eq!(metadata.year, 2024);
        assert_eq!(metadata.model, "");
    }

    #[test]
    fn maps_repeated_year_codes_to_recent_cycle() {
        assert_eq!(decode("5YJXXXXXXNA000001").year, 2022);
    }

    #[test]
    fn unknown_or_short_vins_return_blanks() {
        let metadata = decode("DEMO-HONDA-ACCORD");
        assert_eq!(metadata.make, "");
        assert_eq!(metadata.model, "");
        assert_eq!(metadata.year, 0);
    }
}
