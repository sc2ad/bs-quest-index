use semver::Version;
use std::path::PathBuf;
use tokio::fs;
use tracing_subscriber::fmt::format::FmtSpan;
use warp::http::StatusCode;

use crate::file_repo::FileRepo;

#[tokio::test(flavor = "multi_thread")]
async fn test() {
    tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let database_url = "target/test-database.db".to_owned();
    let downloads_path = PathBuf::from("target/test-downloads");

    fs::remove_file(&database_url).await.ok();
    fs::remove_dir_all(&downloads_path).await.ok();

    let config = Box::leak(Box::new(crate::config::Config {
        port: 0,
        database_url,
        downloads_path,
        log_level: None,
        admin_keys: vec!["admin_password".to_owned()].into_iter().collect(),
    }));
    let pool = crate::db::connect(&config.database_url).await.unwrap();

    let file_repo = Box::leak(Box::new(FileRepo::new(config.downloads_path.clone())));

    let routes = crate::routes::handler(pool, config, file_repo);

    // Upload our mod upload key
    let reply = warp::test::request()
        .path("/publish_key")
        .method("POST")
        .header("Authorization", "admin_password")
        .body(b"{\"user\": \"test\", \"pw\": \"password\"}")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::CREATED);

    // Upload some mods

    let reply = warp::test::request()
        .path("/bshook/1.0.0")
        .method("POST")
        .header("Authorization", "password")
        .body(b"bshook-1.0.0")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::CREATED);

    let reply = warp::test::request()
        .path("/bshook/1.2.0")
        .method("POST")
        .header("Authorization", "password")
        .body(b"bshook-1.2.0")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::CREATED);

    let reply = warp::test::request()
        .path("/hsv/2.3.4")
        .method("POST")
        .header("Authorization", "password")
        .body(b"hsv-2.3.4")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::CREATED);

    // Try uploading a duplicate

    let reply = warp::test::request()
        .path("/bshook/1.0.0")
        .method("POST")
        .header("Authorization", "password")
        .body(b"bshook-1.0.0 number two")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::CONFLICT);

    // Try uploading without a key

    let reply = warp::test::request()
        .path("/hsv/5.0.0")
        .method("POST")
        .body(b"hsv-2.3.4")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::UNAUTHORIZED);

    // Try uploading with an invalid key

    let reply = warp::test::request()
        .path("/hsv/5.0.0")
        .method("POST")
        .header("Authorization", "not password")
        .body(b"hsv-2.3.4")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::UNAUTHORIZED);

    // Download a mod

    let reply = warp::test::request()
        .path("/bshook/1.0.0")
        .method("GET")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::OK);
    assert_eq!(reply.body().as_ref(), b"bshook-1.0.0");

    // Try downloading a mod that doesn't exist

    let reply = warp::test::request()
        .path("/bshook/3.0.0")
        .method("GET")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::NOT_FOUND);

    // Get the latest version of a mod matching the requirements

    let reply = warp::test::request()
        .path("/bshook?req=^1")
        .method("GET")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<'_, crate::db::Mod>(reply.body().as_ref()).unwrap(),
        crate::db::Mod {
            id: "bshook".to_owned(),
            version: Version::new(1, 2, 0)
        }
    );

    // Get all the versions of a mod matching the requirements

    let reply = warp::test::request()
        .path("/bshook?req=^1&limit=0")
        .method("GET")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<'_, Vec<crate::db::Mod>>(reply.body().as_ref()).unwrap(),
        vec![
            crate::db::Mod {
                id: "bshook".to_owned(),
                version: Version::new(1, 2, 0)
            },
            crate::db::Mod {
                id: "bshook".to_owned(),
                version: Version::new(1, 0, 0)
            }
        ]
    );

    // Try to get a version of a mod with requirements that can't match

    let reply = warp::test::request()
        .path("/hsv?req=~3")
        .method("GET")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::NOT_FOUND);

    // List all mods
    let reply = warp::test::request()
        .path("/")
        .method("GET")
        .reply(&routes)
        .await;
    assert_eq!(
        serde_json::from_slice::<'_, Vec<&str>>(reply.body().as_ref()).unwrap(),
        vec!["bshook", "hsv"]
    );

    // Delete mod
    let reply = warp::test::request()
        .path("/hsv/2.3.4")
        .method("DELETE")
        .header("Authorization", "admin_password")
        .body(b"hsv-2.3.4")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::OK);

    // Confirm mod deleted
    let reply = warp::test::request()
        .path("/hsv/2.3.4")
        .method("POST")
        .header("Authorization", "password")
        .body(b"hsv-2.3.4")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::CREATED);

    // Delete key
    let reply = warp::test::request()
        .path("/delete_key")
        .method("POST")
        .header("Authorization", "admin_password")
        .body(b"{\"pw\": \"password\"}")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::OK);

    // Try to add but with a key that was deleted
    let reply = warp::test::request()
        .path("/hsv/2.3.5")
        .method("POST")
        .header("Authorization", "password")
        .body(b"hsv-2.3.5")
        .reply(&routes)
        .await;
    assert_eq!(reply.status(), StatusCode::UNAUTHORIZED);

    // good enough tests for now
}
