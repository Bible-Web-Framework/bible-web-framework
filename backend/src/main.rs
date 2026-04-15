use crate::api::route_not_found;
use crate::bible_data::{DynMultiBibleData, MultiBibleData};
use actix_cors::Cors;
use actix_web::{App, HttpServer, middleware, web};
use bible_data::baked::{bake_bible, load_baked_bible};
use bible_data::expanded::{BibleDataError, MultiExpandedBibleData};
use itertools::Itertools;
use notify_debouncer_full::DebounceEventResult;
use notify_debouncer_full::notify::RecursiveMode;
use sqlx::migrate::MigrateDatabase;
use std::any::Any;
use std::borrow::Cow;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{env, fs, path};
use strum::EnumString;
use tracing::log::Level;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

mod api;
mod bible_data;
mod book_category;
mod book_data;
mod index;
mod reference;
mod reference_encoding;
mod search;
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
    Config(#[from] BibleDataError),
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
    let init_start = Instant::now();

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
        .with_ansi_sanitization(false)
        .init();
    tracing::debug!("Debug logging is enabled");

    let bake_mode = option_var("BIBLE_BAKE")?;

    // if bake_mode == Some(BakeMode::Load) {
    //     let bake_dir = var::<PathBuf>("BAKE_DIR")?;
    //     // TODO: Remove testing code
    //     let eanv = load_baked_bible(&File::open(bake_dir.join("eanv.dat"))?).unwrap(); // TODO don't unwrap
    //     println!("{eanv:#?}");
    //     return Ok(());
    // }

    let (bible_data, _extra_bible_state): (web::Data<DynMultiBibleData>, Box<dyn Any>) =
        if bake_mode == Some(BakeMode::Load) {
            todo!("Load bake data")
        } else {
            let bibles_dir = path::absolute(var::<PathBuf>("BIBLES_DIR")?)?;
            if !bibles_dir.try_exists()? {
                tracing::warn!(
                    "Bibles dir {} doesn't exist. Creating.",
                    bibles_dir.display(),
                );
                fs::create_dir_all(&bibles_dir)?;
            }
            let bible_data = MultiExpandedBibleData::load(
                bibles_dir.clone(),
                var_str("DEFAULT_BIBLE")?,
                var_comma_list("DISABLE_BIBLES")?,
            )?;

            if bake_mode == Some(BakeMode::Generate) {
                let bake_dir = var::<PathBuf>("BAKE_DIR")?;
                tracing::info!(
                    "Baking {} bibles into {}",
                    bible_data.bibles.len(),
                    bake_dir.display(),
                );
                fs::create_dir_all(&bake_dir)?;

                let start_time = Instant::now();
                for (id, bible) in bible_data.bibles {
                    tracing::info!("Baking bible {id}");
                    let writer = BufWriter::new(File::create(bake_dir.join(format!("{id}.dat")))?);
                    bake_bible(&bible, writer).unwrap(); // TODO don't unwrap()
                }
                tracing::info!("Baked bibles in {:?}", start_time.elapsed());
                return Ok(());
            }

            let bible_data = Arc::new(bible_data);

            let usj_watcher = {
                let bible_data = bible_data.clone();
                let mut usj_watcher = notify_debouncer_full::new_debouncer(
                    Duration::from_secs(2),
                    None,
                    move |event: DebounceEventResult| {
                        tracing::debug!("Received file watch event {event:?}");
                        match event {
                            Ok(evs) => {
                                for ev in evs {
                                    if let Err(err) = bible_data.handle_file_change(ev.event) {
                                        tracing::error!(
                                            "Failed to update loaded data from file watch event: {err}"
                                        );
                                    }
                                }
                            }
                            Err(errs) => {
                                for err in errs {
                                    tracing::error!("Error in USJ file watcher: {err}");
                                }
                                if let Err(err) = bible_data.reload_everything() {
                                    tracing::error!("Failed to reload all USJs: {err}");
                                }
                            }
                        };
                    },
                )?;
                usj_watcher.watch(bibles_dir, RecursiveMode::Recursive)?;
                usj_watcher
            };

            let bible_data = bible_data as Arc<DynMultiBibleData>;
            (web::Data::from(bible_data), Box::new(usj_watcher))
        };

    let database_read_only = web::Data::new(DatabaseReadOnly(
        option_var("DATABASE_READ_ONLY")?.unwrap_or_default(),
    ));

    let database = {
        let db_url = var_str("DATABASE_URL")?;
        tracing::info!("Connecting to database {db_url}");
        sqlx::any::install_default_drivers();
        if !database_read_only.0 && !sqlx::Any::database_exists(&db_url).await? {
            tracing::info!("Database doesn't exist, creating new database");
            sqlx::Any::create_database(&db_url).await?;
        }
        let database = sqlx::AnyPool::connect(&db_url).await?;
        sqlx::migrate!().run(&database).await?;
        web::Data::new(database)
    };

    tracing::info!("Finished loading in {:?}", init_start.elapsed());

    let bind_host = var_str("BIND_HOST")?;
    let bind_port = var("BIND_PORT")?;
    HttpServer::new(move || {
        App::new()
            .app_data(bible_data.clone())
            .app_data(database_read_only.clone())
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

pub struct DatabaseReadOnly(bool);

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
    let parsed_value = parse_var_value(var_name, Cow::Owned(base_value))?;
    Ok(parsed_value)
}

fn option_var<T: FromStr>(var_name: impl AsRef<OsStr>) -> Result<Option<T>, ServerError>
where
    T::Err: Error + Send + 'static,
{
    let Ok(base_value) = env::var(&var_name) else {
        return Ok(None);
    };
    let parsed_value = parse_var_value(var_name, Cow::Owned(base_value))?;
    Ok(Some(parsed_value))
}

fn var_comma_list<T, C>(var_name: impl AsRef<OsStr>) -> Result<C, ServerError>
where
    T: FromStr,
    T::Err: Error + Send + 'static,
    C: FromIterator<T> + Default,
{
    env::var(var_name.as_ref())
        .ok()
        .filter(|x| !x.is_empty())
        .map_or_else(
            || Ok(C::default()),
            |x| {
                x.split(',')
                    .map(|base_value| parse_var_value(&var_name, Cow::Borrowed(base_value)))
                    .try_collect()
            },
        )
}

fn parse_var_value<T>(var_name: impl AsRef<OsStr>, base_value: Cow<str>) -> Result<T, ServerError>
where
    T: FromStr,
    T::Err: Error + Send + 'static,
{
    base_value.parse().map_err(|err| {
        ServerError::EnvParse(
            var_name.as_ref().display().to_string(),
            base_value.into_owned(),
            Box::new(err),
        )
    })
}

#[derive(PartialEq, Eq, EnumString)]
#[strum(serialize_all = "snake_case")]
enum BakeMode {
    Generate,
    Load,
}
