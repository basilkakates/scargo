#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumericTransform {
    scale: f64,
    offset: f64,
}

impl NumericTransform {
    const fn new(scale: f64, offset: f64) -> Self {
        Self { scale, offset }
    }

    pub fn apply(self, value: f64) -> f64 {
        (value * self.scale) + self.offset
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelUnitMetadata {
    pub unit_family: &'static str,
    pub canonical_unit: &'static str,
    pub display_units: &'static [&'static str],
    pub default_display_unit: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalMetric {
    pub key: String,
    pub label: String,
    pub storage_unit: Option<String>,
    pub transform: Option<NumericTransform>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetricPolicy {
    pub category: &'static str,
    pub sensitivity: &'static str,
    pub rollup: bool,
    pub public_cohort: bool,
    pub derived_preferred: bool,
}

const SPEED_UNITS: &[&str] = &["mph", "km/h"];
const DISTANCE_UNITS: &[&str] = &["miles", "km"];
const TEMPERATURE_UNITS: &[&str] = &["C", "F"];
const PRESSURE_KPA_UNITS: &[&str] = &["kPa", "psi", "inHg"];
const PRESSURE_PA_UNITS: &[&str] = &["Pa", "inH2O"];
const ACCELERATION_UNITS: &[&str] = &["m/s²", "ft/s²"];
const FUEL_ECONOMY_UNITS: &[&str] = &["MPG", "km/l"];
const FUEL_RATE_UNITS: &[&str] = &["gal/hr", "l/hr"];
const VOLUME_UNITS: &[&str] = &["gal", "l"];
const CO2_RATE_UNITS: &[&str] = &["lb/mile", "g/km"];
const CO2_TOTAL_UNITS: &[&str] = &["kg", "lbs"];
const CO2_FLOW_UNITS: &[&str] = &["g/s", "lb/min"];
const POWER_UNITS: &[&str] = &["hp", "kW"];
const TORQUE_UNITS: &[&str] = &["lb-ft", "N-m"];
const LENGTH_UNITS: &[&str] = &["m", "ft"];
const AIR_FLOW_UNITS: &[&str] = &["g/s", "lb/min"];

const ROLLUP_KEYS: &[&str] = &[
    "absolute_load_value",
    "absolute_throttle_position",
    "absolute_throttle_position_b",
    "accelerator_pedal_position_d",
    "accelerator_pedal_position_e",
    "a_f_actual",
    "a_f_commanded",
    "barometric_pressure",
    "boost",
    "calculated_load_value",
    "catalyst_temperature_bank_1_sensor_1",
    "catalyst_temperature_bank_2_sensor_1",
    "co2_flow",
    "commanded_egr",
    "commanded_evaporative_purge",
    "commanded_throttle_actuator_control",
    "control_module_voltage",
    "distance_traveled_since_dtcs_cleared",
    "distance_traveled_while_mil_is_activated",
    "egr_error",
    "engine_coolant_temperature",
    "engine_power",
    "engine_rpm",
    "engine_torque",
    "evap_system_vapor_pressure",
    "fuel_air_commanded_equivalence_ratio",
    "fuel_level_input",
    "fuel_rate",
    "ignition_timing_advance_for_1_cylinder",
    "instant_co2_rate",
    "instant_fuel_economy",
    "intake_air_temperature",
    "intake_manifold_absolute_pressure",
    "long_term_fuel_trim_bank_1",
    "long_term_fuel_trim_bank_2",
    "long_term_fuel_trim_bank_3",
    "long_term_fuel_trim_bank_4",
    "mass_air_flow_rate",
    "o2_sensor_current_wide_range_bank_1_sensor_1",
    "o2_sensor_current_wide_range_bank_2_sensor_1",
    "o2_voltage_bank_1_sensor_2",
    "o2_voltage_bank_2_sensor_2",
    "relative_throttle_position",
    "short_term_fuel_trim_bank_1",
    "short_term_fuel_trim_bank_1_sensor_2",
    "short_term_fuel_trim_bank_2",
    "short_term_fuel_trim_bank_2_sensor_2",
    "short_term_fuel_trim_bank_3",
    "short_term_fuel_trim_bank_4",
    "time_since_engine_start",
    "total_fuel_economy",
    "vehicle_speed",
];

const PUBLIC_SAE_KEYS: &[&str] = &[
    "absolute_load_value",
    "absolute_throttle_position",
    "absolute_throttle_position_b",
    "accelerator_pedal_position_d",
    "accelerator_pedal_position_e",
    "barometric_pressure",
    "calculated_load_value",
    "catalyst_temperature_bank_1_sensor_1",
    "catalyst_temperature_bank_2_sensor_1",
    "commanded_egr",
    "commanded_evaporative_purge",
    "commanded_throttle_actuator_control",
    "control_module_voltage",
    "distance_traveled_since_dtcs_cleared",
    "distance_traveled_while_mil_is_activated",
    "egr_error",
    "engine_coolant_temperature",
    "engine_rpm",
    "evap_system_vapor_pressure",
    "fuel_level_input",
    "ignition_timing_advance_for_1_cylinder",
    "intake_air_temperature",
    "long_term_fuel_trim_bank_1",
    "long_term_fuel_trim_bank_2",
    "long_term_fuel_trim_bank_3",
    "long_term_fuel_trim_bank_4",
    "o2_sensor_current_wide_range_bank_1_sensor_1",
    "o2_sensor_current_wide_range_bank_2_sensor_1",
    "o2_voltage_bank_1_sensor_2",
    "o2_voltage_bank_2_sensor_2",
    "relative_throttle_position",
    "short_term_fuel_trim_bank_1",
    "short_term_fuel_trim_bank_1_sensor_2",
    "short_term_fuel_trim_bank_2",
    "short_term_fuel_trim_bank_2_sensor_2",
    "short_term_fuel_trim_bank_3",
    "short_term_fuel_trim_bank_4",
    "time_since_engine_start",
    "vehicle_speed",
];

const PUBLIC_FUEL_KEYS: &[&str] = &[
    "co2_flow",
    "fuel_rate",
    "instant_co2_rate",
    "instant_fuel_economy",
    "total_fuel_economy",
];

const OWNER_FUEL_KEYS: &[&str] = &["fuel_remaining", "total_co2"];

const TRIP_KEYS: &[&str] = &[
    "average_speed",
    "average_trip_co2_rate",
    "hard_accel_count",
    "hard_brake_count",
    "idling_count",
    "max_speed",
    "seconds_idling",
    "total_trip_co2",
    "trip_cost",
    "trip_distance",
    "trip_duration",
    "trip_fuel",
    "trip_fuel_economy",
];

const PUBLIC_CALCULATED_KEYS: &[&str] = &[
    "a_f_actual",
    "a_f_commanded",
    "boost",
    "engine_power",
    "engine_torque",
    "fuel_air_commanded_equivalence_ratio",
    "intake_manifold_absolute_pressure",
    "mass_air_flow_rate",
];

const OWNER_CALCULATED_KEYS: &[&str] = &["acceleration", "acceleration_avg", "distance_to_empty"];

const GPS_KEYS: &[&str] = &[
    "altitude",
    "bearing",
    "gps_speed",
    "horz_accuracy",
    "latitude",
    "longitude",
];

const PHONE_SENSOR_KEYS: &[&str] = &[
    "accel_grav_x",
    "accel_grav_y",
    "accel_grav_z",
    "accel_x",
    "accel_y",
    "accel_z",
    "magnetometer_x",
    "magnetometer_y",
    "magnetometer_z",
    "pitch",
    "roll",
    "rotation_rate_x",
    "rotation_rate_y",
    "rotation_rate_z",
];

const ADAPTER_KEYS: &[&str] = &["adapter_voltage"];
const SYSTEM_KEYS: &[&str] = &["pid_refresh_rate"];

const fn linear(scale: f64) -> Option<NumericTransform> {
    Some(NumericTransform::new(scale, 0.0))
}

const fn affine(scale: f64, offset: f64) -> Option<NumericTransform> {
    Some(NumericTransform::new(scale, offset))
}

fn alias_key(base_key: &str) -> &str {
    match base_key {
        "rpm" => "engine_rpm",
        "speed" => "vehicle_speed",
        "map" => "intake_manifold_absolute_pressure",
        "acceleration_x" => "accel_x",
        "acceleration_y" => "accel_y",
        "acceleration_z" => "accel_z",
        _ => base_key,
    }
}

fn metadata_for_key(key: &str) -> Option<ChannelUnitMetadata> {
    match key {
        "vehicle_speed" | "gps_speed" | "max_speed" | "average_speed" => {
            Some(ChannelUnitMetadata {
                unit_family: "speed",
                canonical_unit: "mph",
                display_units: SPEED_UNITS,
                default_display_unit: "mph",
            })
        }
        "trip_distance"
        | "distance_to_empty"
        | "distance_traveled_since_dtcs_cleared"
        | "distance_traveled_while_mil_is_activated" => Some(ChannelUnitMetadata {
            unit_family: "distance",
            canonical_unit: "miles",
            display_units: DISTANCE_UNITS,
            default_display_unit: "miles",
        }),
        "engine_coolant_temperature"
        | "intake_air_temperature"
        | "catalyst_temperature_bank_1_sensor_1"
        | "catalyst_temperature_bank_2_sensor_1" => Some(ChannelUnitMetadata {
            unit_family: "temperature",
            canonical_unit: "c",
            display_units: TEMPERATURE_UNITS,
            default_display_unit: "C",
        }),
        "intake_manifold_absolute_pressure" | "boost" | "barometric_pressure" => {
            Some(ChannelUnitMetadata {
                unit_family: "pressure_kpa",
                canonical_unit: "kpa",
                display_units: PRESSURE_KPA_UNITS,
                default_display_unit: "kPa",
            })
        }
        "evap_system_vapor_pressure" => Some(ChannelUnitMetadata {
            unit_family: "pressure_pa",
            canonical_unit: "pa",
            display_units: PRESSURE_PA_UNITS,
            default_display_unit: "Pa",
        }),
        "acceleration" | "acceleration_avg" | "accel_x" | "accel_y" | "accel_z"
        | "accel_grav_x" | "accel_grav_y" | "accel_grav_z" => Some(ChannelUnitMetadata {
            unit_family: "acceleration",
            canonical_unit: "m s 2",
            display_units: ACCELERATION_UNITS,
            default_display_unit: "m/s²",
        }),
        "instant_fuel_economy" | "total_fuel_economy" | "trip_fuel_economy" => {
            Some(ChannelUnitMetadata {
                unit_family: "fuel_economy",
                canonical_unit: "mpg",
                display_units: FUEL_ECONOMY_UNITS,
                default_display_unit: "MPG",
            })
        }
        "fuel_rate" => Some(ChannelUnitMetadata {
            unit_family: "fuel_rate",
            canonical_unit: "gal hr",
            display_units: FUEL_RATE_UNITS,
            default_display_unit: "gal/hr",
        }),
        "fuel_remaining" | "trip_fuel" => Some(ChannelUnitMetadata {
            unit_family: "volume",
            canonical_unit: "gal",
            display_units: VOLUME_UNITS,
            default_display_unit: "gal",
        }),
        "mass_air_flow_rate" => Some(ChannelUnitMetadata {
            unit_family: "air_flow",
            canonical_unit: "g s",
            display_units: AIR_FLOW_UNITS,
            default_display_unit: "g/s",
        }),
        "instant_co2_rate" | "average_trip_co2_rate" => Some(ChannelUnitMetadata {
            unit_family: "co2_rate",
            canonical_unit: "lb mile",
            display_units: CO2_RATE_UNITS,
            default_display_unit: "lb/mile",
        }),
        "total_co2" | "total_trip_co2" => Some(ChannelUnitMetadata {
            unit_family: "co2_total",
            canonical_unit: "kg",
            display_units: CO2_TOTAL_UNITS,
            default_display_unit: "kg",
        }),
        "co2_flow" => Some(ChannelUnitMetadata {
            unit_family: "co2_flow",
            canonical_unit: "g s",
            display_units: CO2_FLOW_UNITS,
            default_display_unit: "g/s",
        }),
        "engine_power" => Some(ChannelUnitMetadata {
            unit_family: "power",
            canonical_unit: "hp",
            display_units: POWER_UNITS,
            default_display_unit: "hp",
        }),
        "engine_torque" => Some(ChannelUnitMetadata {
            unit_family: "torque",
            canonical_unit: "lb ft",
            display_units: TORQUE_UNITS,
            default_display_unit: "lb-ft",
        }),
        "altitude" | "horz_accuracy" => Some(ChannelUnitMetadata {
            unit_family: "length",
            canonical_unit: "m",
            display_units: LENGTH_UNITS,
            default_display_unit: "m",
        }),
        _ => None,
    }
}

fn canonical_label_for_key(key: &str) -> Option<&'static str> {
    match key {
        "engine_rpm" => Some("Engine RPM"),
        "vehicle_speed" => Some("Vehicle speed"),
        "intake_manifold_absolute_pressure" => Some("Intake manifold absolute pressure"),
        "mass_air_flow_rate" => Some("Mass air flow rate"),
        "fuel_rate" => Some("Fuel Rate"),
        "instant_fuel_economy" => Some("Instant Fuel Economy"),
        "total_fuel_economy" => Some("Total Fuel Economy"),
        "accel_x" => Some("Accel X"),
        "accel_y" => Some("Accel Y"),
        "accel_z" => Some("Accel Z"),
        _ => None,
    }
}

fn canonical_unit_for_single_unit_metric(key: &str) -> Option<&'static str> {
    match key {
        "engine_rpm" => Some("rpm"),
        "absolute_load_value"
        | "absolute_throttle_position"
        | "absolute_throttle_position_b"
        | "accelerator_pedal_position_d"
        | "accelerator_pedal_position_e"
        | "calculated_load_value"
        | "commanded_egr"
        | "commanded_evaporative_purge"
        | "commanded_throttle_actuator_control"
        | "egr_error"
        | "fuel_level_input"
        | "relative_throttle_position"
        | "short_term_fuel_trim_bank_1_sensor_2"
        | "short_term_fuel_trim_bank_2_sensor_2"
        | "short_term_fuel_trim_bank_1"
        | "short_term_fuel_trim_bank_2"
        | "short_term_fuel_trim_bank_3"
        | "short_term_fuel_trim_bank_4"
        | "long_term_fuel_trim_bank_1"
        | "long_term_fuel_trim_bank_2"
        | "long_term_fuel_trim_bank_3"
        | "long_term_fuel_trim_bank_4" => Some("%"),
        "bearing"
        | "ignition_timing_advance_for_1_cylinder"
        | "latitude"
        | "longitude"
        | "roll"
        | "pitch" => Some("deg"),
        "control_module_voltage"
        | "adapter_voltage"
        | "o2_voltage_bank_1_sensor_2"
        | "o2_voltage_bank_2_sensor_2" => Some("v"),
        "o2_sensor_current_wide_range_bank_1_sensor_1"
        | "o2_sensor_current_wide_range_bank_2_sensor_1" => Some("ma"),
        "rotation_rate_x" | "rotation_rate_y" | "rotation_rate_z" => Some("deg s"),
        "magnetometer_x" | "magnetometer_y" | "magnetometer_z" => Some("ut"),
        "time_since_engine_start" | "seconds_idling" => Some("sec"),
        "trip_duration" => Some("min"),
        "pid_refresh_rate" => Some("hz"),
        "fuel_remaining" | "trip_fuel" => Some("gal"),
        _ => None,
    }
}

fn unit_transform(key: &str, unit: Option<&str>) -> Option<Option<NumericTransform>> {
    let unit = unit.or_else(|| canonical_unit_for_single_unit_metric(key));
    match key {
        "engine_rpm" => match unit {
            Some("rpm") => Some(None),
            _ => None,
        },
        "vehicle_speed" | "gps_speed" | "max_speed" | "average_speed" => match unit {
            Some("mph") | None => Some(None),
            Some("km h") | Some("km hr") | Some("kph") | Some("kmh") => Some(linear(0.621_371_192)),
            _ => None,
        },
        "trip_distance"
        | "distance_to_empty"
        | "distance_traveled_since_dtcs_cleared"
        | "distance_traveled_while_mil_is_activated" => match unit {
            Some("miles") | None => Some(None),
            Some("km") => Some(linear(0.621_371_192)),
            _ => None,
        },
        "engine_coolant_temperature"
        | "intake_air_temperature"
        | "catalyst_temperature_bank_1_sensor_1"
        | "catalyst_temperature_bank_2_sensor_1" => match unit {
            Some("c") | None => Some(None),
            Some("f") => Some(affine(5.0 / 9.0, -17.777_777_777_8)),
            _ => None,
        },
        "intake_manifold_absolute_pressure" | "boost" | "barometric_pressure" => match unit {
            Some("kpa") | None => Some(None),
            Some("psi") | Some("psig") => Some(linear(6.894_757_293_2)),
            Some("inhg") | Some("in hg") => Some(linear(3.386_388_157_89)),
            _ => None,
        },
        "evap_system_vapor_pressure" => match unit {
            Some("pa") | None => Some(None),
            Some("inh2o") | Some("in h2o") => Some(linear(248.84)),
            _ => None,
        },
        "acceleration" | "acceleration_avg" | "accel_x" | "accel_y" | "accel_z"
        | "accel_grav_x" | "accel_grav_y" | "accel_grav_z" => match unit {
            Some("m s 2") | Some("m s2") | Some("m s") | None => Some(None),
            Some("ft s 2") | Some("ft s2") | Some("ft s") => Some(linear(0.3048)),
            Some("g") => Some(linear(9.80665)),
            _ => None,
        },
        "instant_fuel_economy" | "total_fuel_economy" | "trip_fuel_economy" => match unit {
            Some("mpg") | None => Some(None),
            Some("km l") => Some(linear(2.352_145_833)),
            _ => None,
        },
        "fuel_rate" => match unit {
            Some("gal hr") | Some("gal h") | None => Some(None),
            Some("l hr") | Some("l h") => Some(linear(0.264_172_052_4)),
            _ => None,
        },
        "fuel_remaining" | "trip_fuel" => match unit {
            Some("gal") | None => Some(None),
            Some("l") => Some(linear(0.264_172_052_4)),
            _ => None,
        },
        "mass_air_flow_rate" => match unit {
            Some("g s") | None => Some(None),
            Some("lb min") => Some(linear(7.559_872_833_3)),
            _ => None,
        },
        "instant_co2_rate" | "average_trip_co2_rate" => match unit {
            Some("lb mile") | None => Some(None),
            Some("g km") => Some(linear(0.005_668_060_9)),
            _ => None,
        },
        "total_co2" | "total_trip_co2" => match unit {
            Some("kg") | None => Some(None),
            Some("lbs") | Some("lb") => Some(linear(0.453_592_37)),
            _ => None,
        },
        "co2_flow" => match unit {
            Some("g s") | None => Some(None),
            Some("lb min") => Some(linear(7.559_872_833_3)),
            _ => None,
        },
        "engine_power" => match unit {
            Some("hp") | None => Some(None),
            Some("kw") => Some(linear(1.341_022_09)),
            _ => None,
        },
        "engine_torque" => match unit {
            Some("lb ft") | None => Some(None),
            Some("n m") => Some(linear(0.737_562_149)),
            _ => None,
        },
        "altitude" | "horz_accuracy" => match unit {
            Some("m") | None => Some(None),
            Some("ft") => Some(linear(0.3048)),
            _ => None,
        },
        _ => canonical_unit_for_single_unit_metric(key).and_then(|expected| {
            if unit == Some(expected) || unit.is_none() {
                Some(None)
            } else {
                None
            }
        }),
    }
}

pub fn canonical_metric(label: &str, base_key: &str, unit: Option<&str>) -> CanonicalMetric {
    let key = alias_key(base_key);
    let unit = unit.map(str::trim).filter(|value| !value.is_empty());
    if let Some(transform) = unit_transform(key, unit) {
        let storage_unit = metadata_for_key(key)
            .map(|metadata| metadata.canonical_unit)
            .or_else(|| canonical_unit_for_single_unit_metric(key));
        return CanonicalMetric {
            key: key.to_string(),
            label: canonical_label_for_key(key)
                .unwrap_or(label.trim())
                .to_string(),
            storage_unit: storage_unit.map(str::to_string),
            transform,
        };
    }

    if let Some(raw_unit) = unit {
        let suffix = raw_unit.replace(' ', "_");
        return CanonicalMetric {
            key: format!("{key}_{suffix}"),
            label: label.trim().to_string(),
            storage_unit: Some(raw_unit.to_string()),
            transform: None,
        };
    }

    CanonicalMetric {
        key: key.to_string(),
        label: label.trim().to_string(),
        storage_unit: None,
        transform: None,
    }
}

pub fn channel_unit_metadata(key: &str) -> Option<ChannelUnitMetadata> {
    metadata_for_key(key)
}

pub fn metric_policy(key: &str) -> MetricPolicy {
    exact_metric_policy(key)
        .or_else(|| duplicate_base_key(key).and_then(exact_metric_policy))
        .unwrap_or(MetricPolicy {
            category: "unknown",
            sensitivity: "owner_only",
            rollup: false,
            public_cohort: false,
            derived_preferred: false,
        })
}

pub fn rollup_metric_keys() -> &'static [&'static str] {
    ROLLUP_KEYS
}

fn exact_metric_policy(key: &str) -> Option<MetricPolicy> {
    if PUBLIC_SAE_KEYS.contains(&key) {
        return Some(MetricPolicy {
            category: "sae_pid",
            sensitivity: "public_vehicle",
            rollup: true,
            public_cohort: true,
            derived_preferred: false,
        });
    }
    if PUBLIC_FUEL_KEYS.contains(&key) {
        return Some(MetricPolicy {
            category: "fuel",
            sensitivity: "public_vehicle",
            rollup: true,
            public_cohort: true,
            derived_preferred: true,
        });
    }
    if OWNER_FUEL_KEYS.contains(&key) {
        return Some(MetricPolicy {
            category: "fuel",
            sensitivity: "owner_only",
            rollup: false,
            public_cohort: false,
            derived_preferred: true,
        });
    }
    if TRIP_KEYS.contains(&key) {
        return Some(MetricPolicy {
            category: "trip",
            sensitivity: "owner_only",
            rollup: false,
            public_cohort: false,
            derived_preferred: true,
        });
    }
    if PUBLIC_CALCULATED_KEYS.contains(&key) {
        return Some(MetricPolicy {
            category: "calculated_pid",
            sensitivity: "public_vehicle",
            rollup: true,
            public_cohort: true,
            derived_preferred: true,
        });
    }
    if OWNER_CALCULATED_KEYS.contains(&key) {
        return Some(MetricPolicy {
            category: "calculated_pid",
            sensitivity: "owner_only",
            rollup: false,
            public_cohort: false,
            derived_preferred: true,
        });
    }
    if GPS_KEYS.contains(&key) {
        return Some(MetricPolicy {
            category: "gps",
            sensitivity: "location",
            rollup: false,
            public_cohort: false,
            derived_preferred: false,
        });
    }
    if PHONE_SENSOR_KEYS.contains(&key) {
        return Some(MetricPolicy {
            category: "sensor",
            sensitivity: "phone_sensor",
            rollup: false,
            public_cohort: false,
            derived_preferred: false,
        });
    }
    if ADAPTER_KEYS.contains(&key) {
        return Some(MetricPolicy {
            category: "adapter",
            sensitivity: "owner_only",
            rollup: false,
            public_cohort: false,
            derived_preferred: false,
        });
    }
    if SYSTEM_KEYS.contains(&key) {
        return Some(MetricPolicy {
            category: "system",
            sensitivity: "owner_only",
            rollup: false,
            public_cohort: false,
            derived_preferred: false,
        });
    }
    None
}

fn duplicate_base_key(key: &str) -> Option<&str> {
    let (base, suffix) = key.rsplit_once('_')?;
    suffix.chars().all(|ch| ch.is_ascii_digit()).then_some(base)
}

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn classifies_representative_metric_policies() {
        assert_eq!(metric_policy("vehicle_speed").category, "sae_pid");
        assert!(metric_policy("vehicle_speed").public_cohort);
        assert_eq!(
            metric_policy("mass_air_flow_rate").category,
            "calculated_pid"
        );
        assert!(metric_policy("mass_air_flow_rate").derived_preferred);
        assert_eq!(metric_policy("latitude").sensitivity, "location");
        assert!(!metric_policy("latitude").rollup);
        assert_eq!(metric_policy("accel_x").sensitivity, "phone_sensor");
        assert_eq!(metric_policy("trip_cost").category, "trip");
        assert!(!metric_policy("trip_cost").public_cohort);
        assert_eq!(metric_policy("fuel_remaining").category, "fuel");
        assert!(!metric_policy("fuel_remaining").rollup);
        assert_eq!(metric_policy("adapter_voltage").category, "adapter");
        assert_eq!(metric_policy("pid_refresh_rate").category, "system");
        assert_eq!(metric_policy("future_ev_metric").category, "unknown");
        assert_eq!(metric_policy("future_ev_metric").sensitivity, "owner_only");
    }

    #[test]
    fn duplicate_suffix_inherits_known_policy() {
        assert_eq!(metric_policy("latitude_2").sensitivity, "location");
        assert!(metric_policy("vehicle_speed_2").rollup);
        assert_eq!(
            metric_policy("short_term_fuel_trim_bank_1").category,
            "sae_pid"
        );
    }

    #[test]
    fn rollup_allowlist_matches_metric_policy() {
        for key in rollup_metric_keys() {
            assert!(metric_policy(key).rollup, "{key} must roll up");
            assert!(metric_policy(key).public_cohort, "{key} must be public");
        }
    }
}
