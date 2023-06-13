use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
// ALso bson::Document
pub struct Post {
    pub id: String,
    pub bucket_id: mongodb::bson::oid::ObjectId,
    pub mime_type: String,
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub url_prefix: String,
}

#[derive(Serialize, Deserialize)]
pub struct Key {
    pub key: String,
}

pub mod handlers {
    use crate::{Config, Key, Post};
    use bytes::BufMut;
    use futures_util::{StreamExt, TryStreamExt};
    use mongodb::{bson::doc, options::ClientOptions, Client};
    use mongodb_gridfs::{options::GridFSBucketOptions, GridFSBucket};
    use rand::distributions::{Alphanumeric, DistString};
    use warp::multipart::Part;

    pub async fn upload(
        form: warp::multipart::FormData,
        api_key: String,
    ) -> Result<impl warp::Reply, warp::Rejection> {
        let connection_env = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let mut mongodb_client_options = ClientOptions::parse(connection_env).await.unwrap();

        mongodb_client_options.app_name = Some("rust-mongodb".to_string());
        let mongodb_client = Client::with_options(mongodb_client_options).unwrap();
        let mongodb_database = mongodb_client.database("ionia-pw");

        let collection = mongodb_database.collection("posts");
        let url_prefix = mongodb_database
            .collection::<Config>("config")
            .find_one(None, None)
            .await
            .unwrap()
            .unwrap_or_else(|| Config {
                url_prefix: "https://i.ionia.pw".to_string(),
            })
            .url_prefix;

        let keys = mongodb_database.collection::<Key>("keys");

        keys.find_one(doc! { "key": api_key }, None)
            .await
            .unwrap()
            .ok_or_else(|| {
                eprintln!("invalid key");
                warp::reject::reject()
            })?;

        let mut parts = form.into_stream();

        while let Ok(p) = parts.next().await.unwrap() {
            match p.name() {
                "data" => {
                    let mime_type = p.content_type().map(|ct| ct.to_string());

                    if mime_type.is_none() {
                        eprintln!("mime type error");
                        warp::reject::reject();
                    }

                    let mime_type = mime_type.unwrap();

                    let value = p
                        .stream()
                        .try_fold(Vec::new(), |mut vec, data| {
                            vec.put(data);
                            async move { Ok(vec) }
                        })
                        .await
                        .map_err(|e| {
                            eprintln!("reading file error: {}", e);
                            warp::reject::reject()
                        })?;

                    let rand_string = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);

                    let mut bucket = GridFSBucket::new(
                        mongodb_database.clone(),
                        Some(GridFSBucketOptions::default()),
                    );
                    let id = bucket
                        .upload_from_stream(rand_string.as_str(), value.as_slice(), None)
                        .await
                        .unwrap();

                    let post = Post {
                        id: rand_string,
                        bucket_id: id,
                        mime_type,
                    };

                    let post_bson = mongodb::bson::to_bson(&post).unwrap();
                    let post_document = match post_bson {
                        mongodb::bson::Bson::Document(document) => document,
                        _ => panic!("Invalid BSON"),
                    };

                    collection.insert_one(post_document, None).await.unwrap();

                    return Ok::<_, warp::Rejection>(format!("{}/{}", url_prefix, post.id));
                }
                _ => {
                    eprintln!("unknown field");
                    return Err(warp::reject::not_found());
                }
            }
        }

        Ok::<_, warp::Rejection>(format!("ok"))
    }

    pub async fn download(id: String) -> Result<impl warp::Reply, warp::Rejection> {
        if id.contains('.') {
            return Err(warp::reject::not_found());
        }

        let connection_env = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let mut mongodb_client_options = ClientOptions::parse(connection_env).await.unwrap();

        mongodb_client_options.app_name = Some("rust-mongodb".to_string());
        let mongodb_client = Client::with_options(mongodb_client_options).unwrap();
        let mongodb_database = mongodb_client.database("ionia-pw");

        let collection = mongodb_database.collection::<Post>("posts");

        let document = collection.find_one(doc! { "id": id }, None).await.unwrap();

        if document.is_none() {
            eprintln!("document not found");
            return Err(warp::reject::not_found());
        }

        let document = document.unwrap();

        let bucket = GridFSBucket::new(
            mongodb_database.clone(),
            Some(GridFSBucketOptions::default()),
        );

        let mut cursor = bucket
            .open_download_stream(document.bucket_id)
            .await
            .unwrap();

        let mut buffer = Vec::new();

        while let Some(item) = cursor.next().await {
            buffer.extend_from_slice(&item);
        }

        let post_bson = mongodb::bson::to_bson(&document).unwrap();

        let post: Post = match post_bson {
            mongodb::bson::Bson::Document(document) => {
                mongodb::bson::from_bson(mongodb::bson::Bson::Document(document)).unwrap()
            }
            _ => panic!("Invalid BSON"),
        };

        Ok::<_, warp::Rejection>(warp::reply::with_header(
            buffer,
            "Content-Type",
            post.mime_type,
        ))
    }
}
