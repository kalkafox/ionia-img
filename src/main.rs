use lilith_upload::handlers;
use tokio::main;
use warp::Filter;

#[main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        println!("DATABASE_URL must be set");
        std::process::exit(1);
    });

    ctrlc::set_handler(|| {
        println!("exiting...");
        std::process::exit(0);
    })?;

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3030".to_string())
        .parse::<u16>()
        .unwrap();

    let upload_route = warp::path!("upload")
        .and(warp::post()) // Only accept POST requests
        .and(warp::multipart::form().max_length(200_000_000)) // 200MB
        .and(warp::header::<String>("x-api-key"))
        .and_then(handlers::upload);

    let download_route = warp::path!(String)
        .and(warp::get())
        .and_then(handlers::download);

    let asset_route = warp::path!("assets" / String)
        .and(warp::get())
        .and_then(|path| async move {
            let web_path =
                std::env::var("ASSETS_PATH").unwrap_or_else(|_| "/frontend/dist".to_string());
            let path = format!("{}/assets/{}", web_path, path);
            // get the mime type from the file extension
            let mime_type = mime_guess::from_path(&path).first_or_octet_stream();

            // read the file into a buffer and serve it
            let file = tokio::fs::read(path).await.unwrap();
            Ok::<_, warp::Rejection>(warp::reply::with_header(
                file,
                "Content-Type",
                mime_type.to_string(),
            ))
        });

    // missed opportunity to name this rooute, but it would be too ridiculous
    let root_route = warp::path::end().and_then(|| async move {
        let web_path =
            std::env::var("ASSETS_PATH").unwrap_or_else(|_| "/frontend/dist".to_string());
        let file = tokio::fs::read_to_string(format!("{}/index.html", web_path))
            .await
            .unwrap();
        Ok::<_, warp::Rejection>(warp::reply::with_header(file, "Content-Type", "text/html"))
    });

    let routes = upload_route
        .or(download_route)
        .or(asset_route)
        .or(root_route);

    let app = warp::serve(routes).run(([0, 0, 0, 0], port));

    println!("Listening on port {}", port);

    app.await;

    Ok(())
}
