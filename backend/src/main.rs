use std::error::Error;
use std::ffi::OsStr;
use std::process;
use std::str::FromStr;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

mod book_data;
mod reference;

#[derive(Debug, thiserror::Error)]
enum ServerError {
    #[error("Environment error for variable {0}: {1}")]
    Env(String, #[source] dotenvy::Error),
    #[error("Invalid value '{1}' for environment variable {0}: {2}")]
    EnvParse(String, String, #[source] Box<dyn Error + Send + 'static>),
    #[error("Invalid logging configuration: {0}")]
    TracingEnv(#[from] tracing_subscriber::filter::FromEnvError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error)
}

fn main() {
    if let Err(e) = real_main() {
        let _ = tracing_subscriber::fmt().try_init();
        tracing::error!("Failed to run server: {e}");
        process::exit(1);
    }
}

fn real_main() -> Result<(), ServerError> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::builder().with_default_directive(LevelFilter::INFO.into()).from_env()?)
        .init();
    let bind_host: String = var("BIND_HOST")?;
    let bind_port: String = var("BIND_PORT")?;
    Ok(())
}

fn var<T: FromStr>(var_name: impl AsRef<OsStr>) -> Result<T, ServerError>
where
    T::Err: Error + Send + 'static
{
    let base_value = dotenvy::var(&var_name)
        .map_err(|x| ServerError::Env(var_name.as_ref().display().to_string(), x))?;
    let parsed_value = base_value.parse()
        .map_err(|x| ServerError::EnvParse(var_name.as_ref().display().to_string(), base_value, Box::new(x)))?;
    Ok(parsed_value)
}
