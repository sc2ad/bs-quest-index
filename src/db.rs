#![allow(clippy::toplevel_ref_arg)]

use futures::{future, StreamExt, TryStreamExt};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::Path;
use tokio::fs;

#[tracing::instrument(level = "info")]
pub async fn connect(url: &str) -> anyhow::Result<&'static SqlitePool> {
    if let Some(dir) = Path::new(url).parent() {
        fs::create_dir_all(dir).await?;
    }
    if fs::metadata(url).await.is_err() {
        fs::write(url, b"").await?;
    }

    let pool = SqlitePool::connect(&format!("sqlite://{}", url)).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(&*Box::leak(Box::new(pool)))
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Mod {
    pub id: String,
    pub version: Version,
}

struct DbMod {
    id: String,

    major: i64,
    minor: i64,
    patch: i64,
}

impl From<DbMod> for Mod {
    fn from(db_mod: DbMod) -> Self {
        Self {
            id: db_mod.id,
            version: Version::new(
                db_mod.major as u64,
                db_mod.minor as u64,
                db_mod.patch as u64,
            ),
        }
    }
}
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct PublishKey {
    pub pw: String,
    pub user: String,
}

struct DbPublishKey {
    pw: String,
    user: String,
}

impl From<DbPublishKey> for PublishKey {
    fn from(db_key: DbPublishKey) -> Self {
        Self {
            pw: db_key.pw,
            user: db_key.user,
        }
    }
}

struct SimpleDbMod {
    id: String,
}

impl Mod {
    pub async fn list(pool: &SqlitePool) -> sqlx::Result<Vec<String>> {
        sqlx::query_as!(SimpleDbMod, "SELECT DISTINCT id FROM mods")
            .fetch(pool)
            .map_ok(|r| r.id)
            .try_collect()
            .await
    }

    pub async fn insert(id: &str, ver: &Version, pool: &SqlitePool) -> sqlx::Result<bool> {
        let major = ver.major as i64;
        let minor = ver.minor as i64;
        let patch = ver.patch as i64;

        let affected = sqlx::query!(
            "INSERT OR IGNORE INTO mods (id, major, minor, patch) VALUES (?, ?, ?, ?)",
            id,
            major,
            minor,
            patch
        )
        .execute(pool)
        .await?;

        if affected.rows_affected() == 0 {
            Ok(false)
        } else {
            Ok(true)
        }
    }

    pub async fn delete(id: &str, ver: &Version, pool: &SqlitePool) -> sqlx::Result<bool> {
        let major = ver.major as i64;
        let minor = ver.minor as i64;
        let patch = ver.patch as i64;

        let affected = sqlx::query!(
            "DELETE FROM mods WHERE id=? AND major=? AND minor=? AND patch=?",
            id,
            major,
            minor,
            patch
        )
        .execute(pool)
        .await?;

        if affected.rows_affected() == 0 {
            Ok(false)
        } else {
            Ok(true)
        }
    }

    pub async fn resolve_one(
        id: &str,
        req: &VersionReq,
        pool: &SqlitePool,
    ) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(
            DbMod,
            "SELECT * FROM mods WHERE id = ? ORDER BY major DESC, minor DESC, patch DESC",
            id
        )
        .fetch(pool)
        .try_filter_map(move |m| Self::tfm_fn(m, req))
        .next()
        .await
        .transpose()
    }

    pub async fn resolve_all(
        id: &str,
        req: &VersionReq,
        pool: &SqlitePool,
    ) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(
            DbMod,
            "SELECT * FROM mods WHERE id = ? ORDER BY major DESC, minor DESC, patch DESC",
            id
        )
        .fetch(pool)
        .try_filter_map(move |m| Self::tfm_fn(m, req))
        .try_collect()
        .await
    }

    pub async fn resolve_n(
        id: &str,
        req: &VersionReq,
        pool: &SqlitePool,
        n: usize,
    ) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as!(
            DbMod,
            "SELECT * FROM mods WHERE id = ? ORDER BY major DESC, minor DESC, patch DESC",
            id
        )
        .fetch(pool)
        .try_filter_map(move |m| Self::tfm_fn(m, req))
        .take(n)
        .try_collect()
        .await
    }

    fn tfm_fn(m: DbMod, req: &VersionReq) -> future::Ready<sqlx::Result<Option<Self>>> {
        let m = Self::from(m);
        if req.matches(&m.version) {
            future::ready(sqlx::Result::Ok(Some(m)))
        } else {
            future::ready(sqlx::Result::Ok(None))
        }
    }
}

impl PublishKey {
    fn tfm_fn(m: DbPublishKey) -> future::Ready<sqlx::Result<Option<Self>>> {
        future::ready(sqlx::Result::Ok(Some(Self::from(m))))
    }

    pub async fn insert(user: &str, pw: &str, pool: &SqlitePool) -> sqlx::Result<bool> {
        let affected = sqlx::query!(
            "INSERT OR IGNORE INTO publish_keys (pw, user) VALUES (?, ?)",
            pw,
            user,
        )
        .execute(pool)
        .await?;

        if affected.rows_affected() == 0 {
            Ok(false)
        } else {
            Ok(true)
        }
    }

    pub async fn resolve_one(key: &str, pool: &SqlitePool) -> sqlx::Result<Option<Self>> {
        sqlx::query_as!(DbPublishKey, "SELECT * FROM publish_keys WHERE pw = ?", key)
            .fetch(pool)
            .try_filter_map(Self::tfm_fn)
            .next()
            .await
            .transpose()
    }

    pub async fn delete_user(user: &str, pool: &SqlitePool) -> sqlx::Result<bool> {
        let affected = sqlx::query!("DELETE FROM publish_keys WHERE user=?", user)
            .execute(pool)
            .await?;

        if affected.rows_affected() == 0 {
            Ok(false)
        } else {
            Ok(true)
        }
    }

    pub async fn delete_pw(pw: &str, pool: &SqlitePool) -> sqlx::Result<bool> {
        let affected = sqlx::query!("DELETE FROM publish_keys WHERE pw=?", pw)
            .execute(pool)
            .await?;

        if affected.rows_affected() == 0 {
            Ok(false)
        } else {
            Ok(true)
        }
    }
}
