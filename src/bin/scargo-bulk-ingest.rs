use scargo::bulk;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = match bulk::parse_args(std::env::args()) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            return std::process::ExitCode::FAILURE;
        }
    };

    match bulk::run(args).await {
        Ok(result) => {
            println!(
                "bulk ingest complete: files_seen={} files_ingested={} duplicates={} failures={} rows_ingested={}",
                result.summary.files_seen,
                result.summary.files_ingested,
                result.summary.files_duplicate,
                result.summary.files_failed,
                result.summary.rows_ingested,
            );
            for failure in &result.failures {
                eprintln!("failed: {} -> {}", failure.path.display(), failure.error);
            }
            if result.failures.is_empty() {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
