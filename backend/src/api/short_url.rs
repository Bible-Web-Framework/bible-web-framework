use crate::api::{ApiError, ApiResult};
use crate::config::BibleConfigLock;
use crate::reference::{BibleReference, parse_references};
use crate::reference_encoding::{
    ReferenceEncodingError, base58_decode, base58_encode, decode_references_from_num,
    encode_references_to_num, is_base58_swear,
};
use actix_web::{get, web};
use actix_web_validator::Query;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use sqlx::SqlitePool;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use validator::Validate;

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Validate)]
pub struct ShortUrl {
    pub r#type: ShortUrlType,
    pub value: ShortUrlValue,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortUrlType {
    Id,
    Encoded,
}

#[derive(Copy, Clone, Debug, SerializeDisplay, DeserializeFromStr)]
pub struct ShortUrlValue(u64);

impl Display for ShortUrlValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&base58_encode(self.0))
    }
}

impl FromStr for ShortUrlValue {
    type Err = ReferenceEncodingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        base58_decode(s).map(ShortUrlValue)
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateShortQueryParams {
    r#ref: String,
}

#[get("/short/create")]
pub async fn short_create(
    query: Query<CreateShortQueryParams>,
    config: web::Data<BibleConfigLock>,
    database: web::Data<SqlitePool>,
) -> ApiResult<web::Json<ShortUrl>> {
    let references = query.into_inner().r#ref;
    let references: Vec<_> = {
        let config = config.read().unwrap();
        parse_references(&references, Some(&config.additional_aliases))
            .into_iter()
            .map(|reference| match reference {
                Ok(r) => {
                    if !config.us.files.contains_key(&r.book) {
                        return Err(ApiError::MissingReference(r));
                    }
                    Ok(r)
                }
                Err(e) => Err(ApiError::InvalidReference(e)),
            })
            .try_collect()?
    };

    let mut transaction = database.begin().await?;

    let references_jsonb = serde_sqlite_jsonb::to_vec(&references)?;
    if let Some(id) = sqlx::query!(
        "SELECT id FROM short_urls WHERE bible_references = $1",
        references_jsonb
    )
    .fetch_optional(&mut *transaction)
    .await?
    {
        return Ok(web::Json(ShortUrl {
            r#type: ShortUrlType::Id,
            value: ShortUrlValue(id.id as u64),
        }));
    };

    match encode_references_to_num(&references) {
        Ok(num) => {
            let id_guess = sqlx::query!("SELECT MAX(id) as max_id FROM short_urls")
                .fetch_one(&mut *transaction)
                .await?
                .max_id
                .unwrap_or(0) as u64
                + 1;
            if num < id_guess && !is_base58_swear(num) {
                return Ok(web::Json(ShortUrl {
                    r#type: ShortUrlType::Encoded,
                    value: ShortUrlValue(num),
                }));
            }
        }
        Err(ReferenceEncodingError::TooBig) => {}
        Err(e) => return Err(ApiError::InvalidReferenceEncoding(e)),
    };

    let mut id = sqlx::query!(
        "INSERT INTO short_urls (bible_references) VALUES ($1) RETURNING id",
        references_jsonb
    )
    .fetch_one(&mut *transaction)
    .await?
    .id as u64;

    const SWEAR_INCREMENT: u64 = 4000; // Slightly above 58^2. Should scramble the last 3 characters.
    while is_base58_swear(id) {
        let new_id = id + SWEAR_INCREMENT;

        let id_i64 = id as i64;
        let new_id_i64 = new_id as i64;
        sqlx::query!(
            "UPDATE short_urls SET id = $2 WHERE id = $1",
            id_i64,
            new_id_i64
        )
        .execute(&mut *transaction)
        .await?;

        id = new_id;
    }

    transaction.commit().await?;

    Ok(web::Json(ShortUrl {
        r#type: ShortUrlType::Id,
        value: ShortUrlValue(id),
    }))
}

#[get("/short/resolve")]
pub async fn short_resolve(
    query: Query<ShortUrl>,
    database: web::Data<SqlitePool>,
) -> ApiResult<web::Json<Vec<BibleReference>>> {
    let short_url = query.into_inner();
    let references = match short_url.r#type {
        ShortUrlType::Id => {
            let value = short_url.value.0 as i64;
            serde_sqlite_jsonb::from_slice(
                &sqlx::query!(
                    "SELECT bible_references FROM short_urls WHERE id = $1",
                    value
                )
                .fetch_optional(&**database)
                .await?
                .ok_or(ApiError::MissingShortReference(short_url.value))?
                .bible_references,
            )?
        }
        ShortUrlType::Encoded => decode_references_from_num(short_url.value.0)?,
    };
    Ok(web::Json(references))
}
