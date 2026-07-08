pub mod canonical;
pub mod csv;
pub mod model;
pub mod vin;

pub use csv::ingest_reader;
pub use model::vin_to_uuid;
