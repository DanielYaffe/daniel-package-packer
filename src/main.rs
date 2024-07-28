mod structs;
use crate::structs::PackageLock;
use clap::Parser;
use futures::StreamExt;
use reqwest::Error as ReqwestError;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::path::PathBuf;
use std::time::Instant;
use std::{collections::HashMap, time::Duration};
use structs::Package;
use tokio::fs;
use tokio::time::sleep;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the package-lock file
    #[arg(short, long)]
    package_lock_path: PathBuf,

    /// Working directory
    #[arg(short, long)]
    working_dir: PathBuf,
}

#[allow(dead_code)]
#[derive(Debug)]
enum ApiError {
    HttpError(reqwest::StatusCode),
    ReqwestError(ReqwestError),
    CustomError(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::HttpError(status) => write!(f, "HTTP error: {}", status),
            ApiError::ReqwestError(err) => write!(f, "Request error: {}", err),
            ApiError::CustomError(msg) => write!(f, "{}", msg),
        }
    }
}

impl Error for ApiError {}

impl From<ReqwestError> for ApiError {
    fn from(err: ReqwestError) -> Self {
        ApiError::ReqwestError(err)
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
    let args = Args::parse();
    let working_dir_metadata = fs::metadata(&args.working_dir).await;
    match working_dir_metadata {
        Ok(value) => {
            if !value.is_dir() || value.permissions().readonly() {
                panic!(
                    "working dir is not of type directory or user does not have permission to edit"
                )
            }
        }
        Err(err) => panic!(
            "error with given working dir {:?}\nError Message: {:?}",
            args.working_dir, err
        ),
    }

    let package_lock_file = File::open(&args.package_lock_path).expect(&format!(
        "File not Found In Path {:?}",
        &args.package_lock_path
    ));
    let package_lock: PackageLock =
        serde_json::from_reader(package_lock_file).expect("JSON was not well-formatted");

    download_all(package_lock.packages, args.working_dir).await;
}

async fn download_all(packages: HashMap<String, Package>, download_dir: PathBuf) {
    let mut tasks_handles = futures::stream::FuturesUnordered::new();
    for (key, value) in packages {
        if key != "" {
            let cloned = download_dir.clone();
            let handle = tokio::spawn(async move {
                if let Err(err) = fs::metadata(&cloned).await {
                    fs::create_dir(&cloned).await.expect(&format!(
                        "failed creating path {:?} \n Error: {:?}",
                        &cloned, err
                    ))
                }

                let package_name = key
                    .rsplit_once("node_modules/")
                    .expect("invalid package Name")
                    .1
                    .split("/")
                    .collect::<Vec<&str>>()
                    .join("-");

                let download_url = value.resolved.clone().expect(&format!(
                    "failed on packge {} failed to get the download url Disclaimer this should not happen",
                    package_name
                ));

                let tarball_name = &cloned.join(format!(
                    "{package_name}-{version}.tgz",
                    version = &value.version,
                    package_name = &package_name
                ));

                let is_package_installed = fs::metadata(&tarball_name);
                if is_package_installed.await.is_err() {
                    let body = fetch_url(&download_url).await.unwrap();

                    let _ = fs::write(&tarball_name, &body).await;
                }
            });
            tasks_handles.push(handle);
        }
    }
    let join_all_time = Instant::now();
    while let Some(item) = tasks_handles.next().await {
        let () = item.unwrap();
    }
    let join_all_time = join_all_time.elapsed().as_nanos();
    println!("completed in {:?}", join_all_time);
}

async fn fetch_url(url: &str) -> Result<bytes::Bytes, ApiError> {
    const MAX_RETRIES: u8 = 3;
    const RETRY_DELAY: Duration = Duration::from_secs(1);

    for attempt in 1..=MAX_RETRIES {
        match reqwest::get(url).await {
            Ok(resp) if resp.status().is_success() => return Ok(resp.bytes().await?),
            Ok(resp) => eprintln!(
                "Attempt {}: Request failed with status: {}",
                attempt,
                resp.status()
            ),
            Err(err) => eprintln!("Attempt {}: Request error: {}", attempt, err),
        }

        if attempt < MAX_RETRIES {
            eprintln!(
                "Retrying in {} seconds... ({}/{})",
                RETRY_DELAY.as_secs(),
                attempt,
                MAX_RETRIES
            );
            sleep(RETRY_DELAY).await;
        }
    }
    Err(ApiError::CustomError(
        "Failed to fetch the URL after multiple attempts".to_string(),
    ))
}
