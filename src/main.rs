use bytes::BufMut;
use futures::TryStreamExt;
use msg_parser::Outlook;
use std::convert::Infallible;
use std::env;
use std::net::Ipv4Addr;
use warp::{
    http::Response,
    http::StatusCode,
    multipart::{FormData, Part},
    Filter, Rejection, Reply,
};

#[tokio::main]
async fn main() {
    // specify constants
    const MAX_UPLOAD_SIZE: u64 = 20_971_520; // limit uploads to 20 MB
    const PORT_KEY: &str = "FUNCTIONS_CUSTOMHANDLER_PORT";

    // specify route parameters
    let api_route = warp::any()
        .and(warp::path("api"))
        .and(warp::path("msg_to_json"))
        .and(warp::path::end())
        .and(warp::multipart::form().max_length(MAX_UPLOAD_SIZE))
        .and_then(upload_handler);

    let routes = api_route.recover(handle_rejection);

    // specify server parameters
    let port: u16 = match env::var(PORT_KEY) {
        Ok(val) => val.parse().expect("Custom Handler port is not a number!"),
        Err(_) => 3000,
    };

    // run api server
    println!("running server on {}:{}", Ipv4Addr::LOCALHOST, port);
    warp::serve(routes).run((Ipv4Addr::LOCALHOST, port)).await;
}

fn read_email(slice: &[u8]) -> String {
    println!("running read_email");

    // create outlook object
    let outlook = Outlook::from_slice(slice).unwrap();

    // flush as json string
    let json_string = match outlook.to_json() {
        Ok(data) => data,
        Err(error) => panic!("problem opening the msg file: {:?}", error),
    };

    json_string
}

async fn upload_handler(form: FormData) -> Result<impl Reply, Rejection> {
    println!("running upload_handler");
    let parts: Vec<Part> = form.try_collect().await.map_err(|e| {
        eprintln!("form error: {}", e);
        warp::reject::reject()
    })?;

    let mut json_string = String::from("{}");

    // loop for each part in multi part upload
    for part in parts {
        println!(
            "[{}] received file \"{}\" with mime type \"{}\"",
            part.name(),
            part.filename().unwrap(),
            part.content_type().unwrap()
        );
        // if multi part key is named as "msg"
        if part.name() == "msg" {
            // check the content type of the file
            match part.content_type() {
                Some(file_type) => match file_type {
                    "application/vnd.ms-outlook" => {
                        println!("outlook message file found");
                    }
                    "application/octet-stream" => {
                        println!("possible outlook message file found");
                    }
                    value => {
                        eprintln!(
                            "invalid file type found: {}, please provide a .msg file",
                            value
                        );
                        return Err(warp::reject::reject());
                    }
                },
                None => {
                    eprintln!("file type could not be determined");
                    return Err(warp::reject::reject());
                }
            }

            // stream contents into memory slice
            let value = part
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

            // read email into json string
            json_string = read_email(&value);
        }
    }

    // build http response for succesful run
    Ok(Response::builder()
        .header("Content-Type", "application/json")
        .body(json_string))
}

async fn handle_rejection(err: Rejection) -> std::result::Result<impl Reply, Infallible> {
    println!("running handle_rejection");
    let (code, message) = if err.is_not_found() {
        (StatusCode::NOT_FOUND, "Not Found".to_string())
    } else if err.find::<warp::reject::PayloadTooLarge>().is_some() {
        (StatusCode::BAD_REQUEST, "Payload too large".to_string())
    } else if err.find::<warp::reject::InvalidHeader>().is_some() {
        (StatusCode::BAD_REQUEST, "{}".to_string())
    } else {
        eprintln!("unhandled error: {:?}", err);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal Server Error".to_string(),
        )
    };
    Ok(warp::reply::with_status(message, code))
}
