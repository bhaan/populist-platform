use super::{
    address::{Address, AddressInput},
    enums::State,
};
use crate::{DateTime, Error};
use async_graphql::{Enum, InputObject};
use geocodio::GeocodioProxy;
use pwhash::bcrypt;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Type};

#[derive(FromRow, Debug, Clone)]
pub struct User {
    pub id: uuid::Uuid,
    pub email: String,
    pub username: String,
    pub password: String,
    pub role: Role,
    pub organization_id: Option<uuid::Uuid>,
    pub confirmed_at: Option<DateTime>,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

#[derive(FromRow, Debug, Clone)]
pub struct UserProfile {
    pub id: uuid::Uuid,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub address_id: Option<uuid::Uuid>,
    pub user_id: uuid::Uuid,
}

#[derive(FromRow, Debug, Clone)]
pub struct UserWithProfile {
    pub id: uuid::Uuid,
    pub email: String,
    pub username: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub profile_picture_url: Option<String>,
}

#[derive(Serialize, Deserialize, InputObject)]
pub struct CreateUserInput {
    #[graphql(validator(email))]
    pub email: String,
    pub username: String,
    pub password: String,
    pub role: Option<Role>,
    pub organization_id: Option<uuid::Uuid>,
}

#[derive(Serialize, Deserialize, InputObject, Debug)]
pub struct CreateUserWithProfileInput {
    #[graphql(validator(email))]
    pub email: String,
    pub username: String,
    pub password: String,
    pub address: AddressInput,
    pub confirmation_token: String,
}

#[derive(
    Debug, Clone, strum_macros::Display, Type, Serialize, Deserialize, Copy, Eq, PartialEq, Enum,
)]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Role {
    SUPERUSER,
    STAFF,
    PREMIUM,
    BASIC,
}

impl User {
    pub async fn create(db_pool: &PgPool, input: &CreateUserInput) -> Result<Self, Error> {
        let hash = bcrypt::hash(&input.password).unwrap();
        let role = input.role.unwrap_or(Role::BASIC);
        let record = sqlx::query_as!(
            User,
            r#" 
            WITH ins_user AS (
                INSERT INTO populist_user (email, username, password, role, organization_id)
                VALUES (LOWER($1), LOWER($2), $3, $4, $5)
                RETURNING *
            ), 
            ins_profile AS (
                INSERT INTO user_profile (user_id)
                SELECT id FROM ins_user
            )
            SELECT id, email, username, password, role AS "role:Role", organization_id, created_at, confirmed_at, updated_at FROM ins_user
            "#,
            input.email,
            input.username,
            hash,
            role as Role,
            input.organization_id
        ).fetch_one(db_pool).await?;

        Ok(record)
    }

    pub async fn create_with_profile(
        db_pool: &PgPool,
        input: &CreateUserWithProfileInput,
    ) -> Result<Self, Error> {
        let hash = bcrypt::hash(&input.password).unwrap();
        let lon = input
            .address
            .coordinates
            .as_ref()
            .map(|c| c.longitude)
            .unwrap();
        let lat = input
            .address
            .coordinates
            .as_ref()
            .map(|c| c.latitude)
            .unwrap();
        let record = sqlx::query_as!(
            User,
            r#"
                WITH ins_user AS (
                    INSERT INTO populist_user (email, username, password, role, confirmation_token)
                    VALUES (LOWER($1), LOWER($2), $3, $4, $5)
                    RETURNING id, email, username, password, role AS "role:Role", organization_id, created_at, confirmed_at, updated_at
                ),
                ins_address AS (
                    INSERT INTO address (line_1, line_2, city, state, county, country, postal_code, lon, lat, geog, geom, congressional_district, state_senate_district, state_house_district)
                    VALUES ($6, $7, $8, $9, $10, $11, $12, $13, $14, ST_SetSRID(ST_MakePoint($13, $14), 4326), ST_GeomFromText($15, 4326), $16, $17, $18)
                    RETURNING id
                ),
                ins_profile AS (
                    INSERT INTO user_profile (address_id, user_id)
                    VALUES ((SELECT id FROM ins_address), (SELECT id FROM ins_user))
                )
                SELECT ins_user.* FROM ins_user
            "#,
            input.email,
            input.username,
            hash,
            Role::BASIC as Role,
            input.confirmation_token,
            input.address.line_1,
            input.address.line_2,
            input.address.city,
            input.address.state.to_string(),
            input.address.county,
            input.address.country,
            input.address.postal_code,
            lon,
            lat,
            format!("POINT({} {})", lon, lat), // A string we pass into ST_GeomFromText function
            input.address.congressional_district,
            input.address.state_senate_district,
            input.address.state_house_district,
        ).fetch_one(db_pool).await?;

        // Need to handle case of existing user

        Ok(record)
    }

    pub async fn find_by_id(db_pool: &PgPool, id: uuid::Uuid) -> Result<Self, Error> {
        let record = sqlx::query_as!(
            User,
            r#"
                SELECT id, email, username, password, role AS "role:Role", organization_id, created_at, confirmed_at, updated_at FROM populist_user 
                WHERE $1 = id;
            "#,
            id
        ).fetch_optional(db_pool).await?;

        match record {
            Some(record) => Ok(record),
            None => Err(Error::EmailOrUsernameNotFound),
        }
    }

    pub async fn find_by_email_or_username(
        db_pool: &PgPool,
        email_or_username: String,
    ) -> Result<Self, Error> {
        let record = sqlx::query_as!(
            User,
            r#"
                SELECT 
                    id, 
                    email, 
                    username, 
                    password, 
                    role AS "role:Role", 
                    organization_id,
                    created_at, 
                    confirmed_at, 
                    updated_at 
                FROM populist_user 
                WHERE LOWER($1) IN(email, username);
            "#,
            email_or_username
        )
        .fetch_optional(db_pool)
        .await?;

        match record {
            Some(record) => Ok(record),
            None => Err(Error::EmailOrUsernameNotFound),
        }
    }

    pub async fn validate_email_exists(db_pool: &PgPool, email: String) -> Result<bool, Error> {
        let existing_user = sqlx::query!(
            r#"
            SELECT id FROM populist_user WHERE email = LOWER($1)
        "#,
            email
        )
        .fetch_optional(db_pool)
        .await?;

        if let Some(_user) = existing_user {
            Ok(true)
        } else {
            Err(Error::EmailOrUsernameNotFound)
        }
    }

    pub async fn set_last_login_at(db_pool: &PgPool, id: uuid::Uuid) -> Result<Self, Error> {
        let record = sqlx::query_as!(
            User,
            r#"
                UPDATE populist_user
                SET last_login_at = now()
                WHERE id = $1
                RETURNING id, email, username, password, role AS "role:Role", organization_id, created_at, confirmed_at, updated_at
            "#,
            id
        )
        .fetch_one(db_pool)
        .await?;

        Ok(record)
    }

    pub async fn reset_password(
        db_pool: &PgPool,
        new_password: String,
        reset_token: String,
    ) -> Result<Self, Error> {
        let hash = bcrypt::hash(&new_password).unwrap();

        let update_result = sqlx::query_as!(User,
            r#"
                UPDATE populist_user
                SET password = $1,
                    reset_token = NULL
                WHERE reset_token = $2
                AND reset_token_expires_at > now()
                RETURNING id, email, username, password, role AS "role:Role", organization_id, created_at, confirmed_at, updated_at
            "#,
            hash,
            reset_token
        )
        .fetch_optional(db_pool)
        .await;

        if let Ok(Some(user)) = update_result {
            Ok(user)
        } else {
            Err(Error::ResetTokenInvalid)
        }
    }

    pub async fn update_password(
        db_pool: &PgPool,
        new_password: String,
        user_id: uuid::Uuid,
    ) -> Result<bool, Error> {
        let hash = bcrypt::hash(&new_password).unwrap();

        let update_result = sqlx::query!(
            r#"
                UPDATE populist_user
                SET password = $1
                WHERE id = $2
            "#,
            hash,
            user_id
        )
        .execute(db_pool)
        .await;

        if update_result.is_ok() {
            Ok(true)
        } else {
            Err(Error::Custom(
                "Your password could not be updated".to_string(),
            ))
        }
    }

    pub async fn update_address(
        db_pool: &PgPool,
        user_id: uuid::Uuid,
        address: AddressInput,
    ) -> Result<Address, Error> {
        let geocodio = GeocodioProxy::new().unwrap();
        let address_clone = address.clone();
        let geocode_result = geocodio
            .geocode(
                geocodio::AddressParams::AddressInput(geocodio::AddressInput {
                    line_1: address_clone.line_1,
                    line_2: address_clone.line_2,
                    city: address_clone.city,
                    state: address_clone.state.to_string(),
                    country: address_clone.country,
                    postal_code: address_clone.postal_code,
                }),
                Some(&["cd", "stateleg"]),
            )
            .await;

        match geocode_result {
            Ok(geocodio_data) => {
                let coordinates = geocodio_data.results[0].location.clone();
                let county = geocodio_data.results[0].address_components.county.clone();
                let primary_result = geocodio_data.results[0].fields.as_ref().unwrap();
                let congressional_district =
                    primary_result.congressional_districts.as_ref().unwrap()[0]
                        .district_number
                        .to_string();
                let state_legislative_districts =
                    primary_result.state_legislative_districts.as_ref().unwrap();
                let state_house_district = &state_legislative_districts.house[0].district_number;
                let state_senate_district = &state_legislative_districts.senate[0].district_number;
                let lat = coordinates.latitude;
                let lon = coordinates.longitude;

                let updated_record_result = sqlx::query_as!(
                    Address,
                    r#"
                    INSERT INTO address (id, line_1, line_2, city, county, state, postal_code, country, lon, lat, geog, geom, congressional_district, state_house_district, state_senate_district)
                    VALUES(
                        COALESCE((SELECT address_id FROM user_profile WHERE user_id = $1), gen_random_uuid()), 
                        $2, $3, $4, $6, $5, $7, $8, $9, $10, ST_SetSRID (ST_MakePoint ($9, $10), 4326), ST_GeomFromText($11, 4326), $12, $13, $14) ON CONFLICT (id)
                    DO UPDATE SET
                        line_1 = EXCLUDED.line_1,
                        line_2 = EXCLUDED.line_2,
                        city = EXCLUDED.city,
                        county = EXCLUDED.county,
                        state = EXCLUDED.state,
                        postal_code = EXCLUDED.postal_code,
                        country = EXCLUDED.country,
                        lon = EXCLUDED.lon,
                        lat = EXCLUDED.lat,
                        geog = EXCLUDED.geog,
                        geom = EXCLUDED.geom,
                        congressional_district = EXCLUDED.congressional_district,
                        state_house_district = EXCLUDED.state_house_district,
                        state_senate_district = EXCLUDED.state_senate_district
                    RETURNING
                        id,
                        line_1,
                        line_2,
                        city,
                        state AS "state:State",
                        postal_code,
                        country,
                        county,
                        congressional_district,
                        state_senate_district,
                        state_house_district
                "#,
                    user_id,
                    address.line_1,
                    address.line_2,
                    address.city,
                    address.state as State,
                    county,
                    address.postal_code,
                    address.country,
                    lon,
                    lat,
                    format!("POINT({} {})", lon, lat), // A string we pass into ST_GeomFromText function
                    Some(congressional_district),
                    state_house_district,
                    state_senate_district,
                )
                .fetch_one(db_pool)
                .await;

                match updated_record_result {
                    Ok(updated_record) => {
                        let _ = sqlx::query!(
                            r#"
                            UPDATE user_profile
                            SET address_id = $1
                            WHERE user_id = $2
                        "#,
                            updated_record.id,
                            user_id
                        )
                        .execute(db_pool)
                        .await;
                        Ok(updated_record)
                    }
                    Err(err) => Err(err.into()),
                }
            }
            Err(_) => Err(Error::Custom(
                "This is not a valid voting address".to_string(),
            )),
        }
    }
}
