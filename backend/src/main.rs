use crate::api::route_not_found;
use crate::config::BibleConfig;
use crate::index::{BibleIndex, ReindexType};
use actix_web::middleware::Logger;
use actix_web::{App, HttpServer, web};
use notify_debouncer_full::DebounceEventResult;
use notify_debouncer_full::notify::RecursiveMode;
use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::RwLock;
use std::time::Duration;
use tracing::log::Level;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

mod api;
mod book_data;
mod config;
mod index;
mod reference;
mod reference_encoding;
mod search;
mod usfm_converter;
mod usj;
mod utils;
mod verse_range;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Environment error for variable {0}: {1}")]
    Env(String, #[source] dotenvy::Error),
    #[error("Invalid value '{1}' for environment variable {0}: {2}")]
    EnvParse(String, String, #[source] Box<dyn Error + Send + 'static>),
    #[error("Invalid logging configuration: {0}")]
    TracingEnv(#[from] tracing_subscriber::filter::ParseError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("File watcher error: {0}")]
    Notify(#[from] notify_debouncer_full::notify::Error),
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
                .parse(var_str("RUST_LOG").unwrap_or_default())?,
        )
        .init();
    tracing::debug!("Debug logging is enabled");

    let us_dir = var::<PathBuf>("US_DIRECTORY")?;
    let bible_config = BibleConfig::load_initial(us_dir.clone())?;

    let mut bible_index = BibleIndex::new();
    bible_index.update_index(ReindexType::FullReindex, &bible_config.us.files);

    let bible_config = web::Data::new(RwLock::new(bible_config));
    let bible_index = web::Data::new(RwLock::new(bible_index));

    let usj_watcher = {
        let config = bible_config.clone();
        let index = bible_index.clone();
        let mut usj_watcher = notify_debouncer_full::new_debouncer(
            Duration::from_secs(2),
            None,
            move |event: DebounceEventResult| {
                tracing::debug!("Received file watch event {event:?}");
                match event {
                    Ok(evs) => {
                        let mut config = config.write().unwrap();
                        for ev in evs {
                            match config.us.handle_file_change(ev.event) {
                                Ok(reindex) => {
                                    if reindex != ReindexType::NoReindex {
                                        let mut index = index.write().unwrap();
                                        index.update_index(reindex, &config.us.files);
                                    }
                                }
                                Err(err) => {
                                    tracing::error!(
                                        "Failed to update loaded USJs from file watch event: {err}"
                                    );
                                }
                            }
                        }
                    }
                    Err(errs) => {
                        for err in errs {
                            tracing::error!("Error in USJ file watcher: {err}");
                        }
                        if let Err(err) = config.write().unwrap().us.reload_all_from_dir() {
                            tracing::error!("Failed to reload all USJs: {err}");
                        }
                    }
                };
            },
        )?;
        usj_watcher.watch(us_dir, RecursiveMode::NonRecursive)?;
        web::Data::new(usj_watcher)
    };

    let bind_host = var_str("BIND_HOST")?;
    let bind_port = var("BIND_PORT")?;
    HttpServer::new(move || {
        App::new()
            .app_data(bible_config.clone())
            .app_data(bible_index.clone())
            .app_data(usj_watcher.clone())
            .wrap(Logger::default().log_level(Level::Debug))
            .default_service(web::to(route_not_found))
            .service(
                web::scope("/v1")
                    .service(api::book)
                    .service(api::search)
                    .service(api::index_route),
            )
    })
    .bind((bind_host, bind_port))?
    .run()
    .await?;
    Ok(())
}

fn var_str(var_name: impl AsRef<OsStr>) -> Result<String, ServerError> {
    let value = dotenvy::var(&var_name)
        .map_err(|x| ServerError::Env(var_name.as_ref().display().to_string(), x))?;
    Ok(value)
}

fn var<T: FromStr>(var_name: impl AsRef<OsStr>) -> Result<T, ServerError>
where
    T::Err: Error + Send + 'static,
{
    let base_value = var_str(&var_name)?;
    let parsed_value = base_value.parse().map_err(|x| {
        ServerError::EnvParse(
            var_name.as_ref().display().to_string(),
            base_value,
            Box::new(x),
        )
    })?;
    Ok(parsed_value)
}
