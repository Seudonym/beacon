use axum::{
    Form,
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use axum_login::{AuthSession, AuthUser, AuthnBackend};
use serde::Deserialize;
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
}

impl AuthUser for User {
    type Id = String;
    fn id(&self) -> Self::Id {
        self.id.clone()
    }
    fn session_auth_hash(&self) -> &[u8] {
        self.password_hash.as_bytes()
    }
}

#[derive(Clone, Deserialize)]
pub struct Credentials {
    username: String,
    password: String,
}

#[derive(Clone)]
pub struct Backend {
    pub db: SqlitePool,
}

impl AuthnBackend for Backend {
    type Credentials = Credentials;
    type Error = sqlx::Error;
    type User = User;
    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user = sqlx::query_as::<_, User>(
            "select id, username, password_hash from users where username = ?",
        )
        .bind(&creds.username)
        .fetch_optional(&self.db)
        .await?;

        let Some(user) = user else {
            return Ok(None);
        };

        let valid = password_auth::verify_password(&creds.password, &user.password_hash).is_ok();

        Ok(valid.then_some(user))
    }

    async fn get_user(
        &self,
        user_id: &axum_login::UserId<Self>,
    ) -> Result<Option<Self::User>, Self::Error> {
        sqlx::query_as::<_, User>("select id, username, password_hash from users where id = ?")
            .bind(user_id)
            .fetch_optional(&self.db)
            .await
    }
}

pub type AppAuthSession = AuthSession<Backend>;

pub async fn register(
    mut auth: AppAuthSession,
    Form(creds): Form<Credentials>,
) -> impl IntoResponse {
    let user = User {
        id: Uuid::new_v4().to_string(),
        username: creds.username,
        password_hash: password_auth::generate_hash(&creds.password),
    };

    let result = sqlx::query("insert into users (id, username, password_hash) values (?, ?, ?)")
        .bind(&user.id)
        .bind(&user.username)
        .bind(&user.password_hash)
        .execute(&auth.backend.db)
        .await;

    match result {
        Ok(_) => {}
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
            return StatusCode::CONFLICT.into_response();
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if auth.login(&user).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    Redirect::to("/me").into_response()
}

pub async fn login(mut auth: AppAuthSession, Form(creds): Form<Credentials>) -> impl IntoResponse {
    let user = match auth.authenticate(creds).await {
        Ok(Some(user)) => user,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if auth.login(&user).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    Redirect::to("/me").into_response()
}

pub async fn logout(mut auth: AppAuthSession) -> impl IntoResponse {
    if auth.logout().await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn me(auth: AppAuthSession) -> impl IntoResponse {
    match auth.user {
        Some(user) => format!("Hello {}", user.username).into_response(),
        None => return StatusCode::UNAUTHORIZED.into_response(),
    }
}
