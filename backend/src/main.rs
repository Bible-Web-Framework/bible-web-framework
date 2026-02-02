use crate::api::route_not_found;
use crate::bible_data::{ConfigError, MultiBibleData};
use crate::index::BibleIndex;
use actix_cors::Cors;
use actix_web::{App, HttpServer, middleware, web};
use sqlx::migrate::MigrateDatabase;
use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::RwLock;
use tracing::log::Level;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

mod api;
mod bible_data;
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
    #[error("Error loading .env: {0}")]
    DotenvError(#[source] dotenvy::Error),
    #[error("Environment error for variable {0}: {1}")]
    Env(String, #[source] env::VarError),
    #[error("Invalid value '{1}' for environment variable {0}: {2}")]
    EnvParse(String, String, #[source] Box<dyn Error + Send + 'static>),
    #[error("Invalid logging configuration: {0}")]
    TracingEnv(#[from] tracing_subscriber::filter::FromEnvError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("File watcher error: {0}")]
    Notify(#[from] notify_debouncer_full::notify::Error),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Database migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
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
    if let Err(e) = dotenvy::dotenv()
        && !e.not_found()
    {
        return Err(ServerError::DotenvError(e));
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()?,
        )
        .init();
    tracing::debug!("Debug logging is enabled");

    let bibles_dir = var::<PathBuf>("BIBLES_DIR")?;
    let bible_data = MultiBibleData::load(bibles_dir.clone())?;

    let bible_index = BibleIndex::new();
    // bible_index.update_index(ReindexType::FullReindex, &bible_config.us.files);

    let bible_config = web::Data::new(bible_data);
    let bible_index = web::Data::new(RwLock::new(bible_index));

    let usj_watcher = {
        // let config = bible_config.clone();
        // let index = bible_index.clone();
        // let mut usj_watcher = notify_debouncer_full::new_debouncer(
        //     Duration::from_secs(2),
        //     None,
        //     move |event: DebounceEventResult| {
        //         tracing::debug!("Received file watch event {event:?}");
        //         match event {
        //             Ok(evs) => {
        //                 let mut config = config.write().unwrap();
        //                 for ev in evs {
        //                     match config.us.handle_file_change(ev.event) {
        //                         Ok(reindex) => {
        //                             if reindex != ReindexType::NoReindex {
        //                                 let mut index = index.write().unwrap();
        //                                 index.update_index(reindex, &config.us.files);
        //                             }
        //                         }
        //                         Err(err) => {
        //                             tracing::error!(
        //                                 "Failed to update loaded USJs from file watch event: {err}"
        //                             );
        //                         }
        //                     }
        //                 }
        //             }
        //             Err(errs) => {
        //                 for err in errs {
        //                     tracing::error!("Error in USJ file watcher: {err}");
        //                 }
        //                 if let Err(err) = config.write().unwrap().us.reload_all_from_dir() {
        //                     tracing::error!("Failed to reload all USJs: {err}");
        //                 }
        //             }
        //         };
        //     },
        // )?;
        // usj_watcher.watch(bibles_dir, RecursiveMode::NonRecursive)?;
        // web::Data::new(usj_watcher)
        web::Data::new(())
    };

    let database = {
        let db_url = var_str("DATABASE_URL")?;
        if !sqlx::Sqlite::database_exists(&db_url).await? {
            tracing::info!("Database {db_url} doesn't exist, creating new database");
            sqlx::Sqlite::create_database(&db_url).await?;
        }
        let database = sqlx::SqlitePool::connect(&db_url).await?;
        sqlx::migrate!().run(&database).await?;
        web::Data::new(database)
    };

    let bind_host = var_str("BIND_HOST")?;
    let bind_port = var("BIND_PORT")?;
    HttpServer::new(move || {
        App::new()
            .app_data(bible_config.clone())
            .app_data(bible_index.clone())
            .app_data(usj_watcher.clone())
            .app_data(database.clone())
            .wrap(Cors::permissive())
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default().log_level(Level::Debug))
            .default_service(web::to(route_not_found))
            .service(api::scope())
    })
    .bind((bind_host, bind_port))?
    .run()
    .await?;
    Ok(())
}

fn var_str(var_name: impl AsRef<OsStr>) -> Result<String, ServerError> {
    let value = env::var(&var_name)
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
