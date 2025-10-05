use actix_web::{App, HttpResponse, HttpServer, Responder};
use std::error::Error;
use std::ffi::OsStr;
use std::process::ExitCode;
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
    Io(#[from] std::io::Error),
}

#[actix_web::main]
async fn main() -> ExitCode {
    match real_main().await {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = tracing_subscriber::fmt().try_init();
            tracing::error!("Failed to run server: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn real_main() -> Result<(), ServerError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()?,
        )
        .init();
    let bind_host: String = var("BIND_HOST")?;
    let bind_port = var("BIND_PORT")?;
    HttpServer::new(|| App::new().service(ping))
        .bind((bind_host, bind_port))?
        .run()
        .await?;
    Ok(())
}

fn var<T: FromStr>(var_name: impl AsRef<OsStr>) -> Result<T, ServerError>
where
    T::Err: Error + Send + 'static,
{
    let base_value = dotenvy::var(&var_name)
        .map_err(|x| ServerError::Env(var_name.as_ref().display().to_string(), x))?;
    let parsed_value = base_value.parse().map_err(|x| {
        ServerError::EnvParse(
            var_name.as_ref().display().to_string(),
            base_value,
            Box::new(x),
        )
    })?;
    Ok(parsed_value)
}

#[actix_web::get("/ping")]
async fn ping() -> impl Responder {
    HttpResponse::NoContent()
}
