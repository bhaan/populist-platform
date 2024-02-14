use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use auth::{jwt, AccessTokenClaims};
use axum::{
    extract::Extension,
    http::HeaderMap,
    response::{self, IntoResponse},
};
use graphql::PopulistSchema;
use jsonwebtoken::TokenData;
use tower_cookies::{Cookie, Cookies};

async fn refresh_token_check(cookies: Cookies) -> Option<TokenData<AccessTokenClaims>> {
    if let Some(refresh_cookie) = cookies.get("refresh_token") {
        match jwt::validate_refresh_token(refresh_cookie.value()) {
            Ok(token_data) => {
                let db_pool = db::pool().await;
                let user = db::User::find_by_id(&db_pool.connection, token_data.claims.sub).await;

                if let Ok(user) = user {
                    // Ensure the refresh token in the cookie matches the one associated with the user in the database
                    if user.clone().refresh_token.unwrap() != refresh_cookie.value() {
                        // If not, remove the cookies and return None
                        cookies.to_owned().remove(Cookie::named("access_token"));
                        cookies.to_owned().remove(Cookie::named("refresh_token"));
                        None
                    } else {
                        // If so, create a new access token, set it in the cookie, and return it
                        let access_token = jwt::create_access_token_for_user(user).unwrap();
                        let mut cookie =
                            tower_cookies::Cookie::new("access_token", access_token.clone());
                        cookie.set_expires(
                            time::OffsetDateTime::now_utc() + time::Duration::hours(24),
                        );
                        cookies.add(cookie);
                        Some(jwt::validate_access_token(&access_token).unwrap())
                    }
                } else {
                    cookies.to_owned().remove(Cookie::named("access_token"));
                    cookies.to_owned().remove(Cookie::named("refresh_token"));
                    None
                }
            }
            Err(_) => {
                cookies.to_owned().remove(Cookie::named("access_token"));
                cookies.to_owned().remove(Cookie::named("refresh_token"));
                None
            }
        }
    } else {
        None
    }
}

pub async fn graphql_handler(
    headers: HeaderMap,
    cookies: Cookies,
    schema: Extension<PopulistSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let mut headers = headers.clone();
    headers.insert("Access-Control-Allow-Credentials", "true".parse().unwrap());

    let bearer_token = headers
        .get("authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.split_whitespace().nth(1));

    let bearer_token_data = if let Some(token) = bearer_token {
        let token_data = jwt::validate_access_token(token);
        if let Ok(token_data) = token_data {
            Some(token_data)
        } else {
            None
        }
    } else {
        None
    };

    let cookie_token_data = match cookies.get("access_token") {
        Some(access_cookie) => match jwt::validate_access_token(access_cookie.value()) {
            Ok(token_data) => Some(token_data),
            Err(_) => refresh_token_check(cookies).await,
        },
        None => refresh_token_check(cookies).await,
    };

    // Use the bearer token if it's present, otherwise use the cookie
    let token_data = bearer_token_data.or(cookie_token_data);

    schema
        .execute(req.into_inner().data(token_data))
        .await
        .into()
}

pub async fn graphql_playground() -> impl IntoResponse {
    response::Html(playground_source(
        GraphQLPlaygroundConfig::new("/").subscription_endpoint("/ws"),
    ))
}
