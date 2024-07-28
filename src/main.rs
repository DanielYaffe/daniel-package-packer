mod structs;
use crate::structs::PackageLock;
use clap::Parser;
use futures::StreamExt;
use reqwest::Error as ReqwestError;
use semver::{Version, VersionReq};
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

macro_rules! merge_option_hashmaps {
    ($($map:expr),*) => {{
        let mut result = HashMap::new();

        $(
            if let Some(map) = $map {
                result.extend(map);
            }
        )*

        result
    }};
}

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]

// #[tokio::main]
async fn main() {
    let args = Args::parse();
    let working_dir_metadata = fs::metadata(&args.working_dir).await;
    match working_dir_metadata {
        Ok(value) => {
            if !value.is_dir() || value.permissions().readonly() {
                panic!("working dir is not of type directory")
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
    // let root_packages = package_lock
    //     .packages
    //     .get("")
    //     .expect("no Root Packages Found");

    // let maps: [&Option<HashMap<String, String>>; 2] =
    //     [&root_packages.dependencies, &root_packages.dev_dependencies];
    // let mut dependencies: Vec<&String> = maps
    //     .into_iter()
    //     .flatten()
    //     .flat_map(|map| map.keys())
    //     .collect();
    // // dependencies.sort();
    // for root_dependency in dependencies {
    //     let download_dir = if root_dependency.contains("/") {
    //         let new_download_dir_name =
    //             &root_dependency.split("/").collect::<Vec<&str>>().join("-");
    //         let working_dir = &args.working_dir;
    //         working_dir.join(new_download_dir_name)
    //     } else {
    //         let working_dir = &args.working_dir;
    //         working_dir.join(&root_dependency)
    //     };
    //     get_package_dependencies(
    //         None,
    //         None,
    //         root_dependency,
    //         None,
    //         &download_dir,
    //         &package_lock,
    //     );
    // }
}

async fn download_all(packages: HashMap<String, Package>, download_dir: PathBuf) {
    let mut tasks_handles = futures::stream::FuturesUnordered::new();
    for (key, value) in packages {
        if key != "" {
            let cloned = download_dir.clone();
            let handle = tokio::spawn(async move {
                let retries = 0;
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
                    // let body = resp.bytes().await.expect("body invalid");
                    let _ = fs::write(&tarball_name, &body).await;
                    // let root_folder = cloned
                    //     .file_name()
                    //     .and_then(|os_str| os_str.to_str())
                    //     .expect("should never happen");

                    // println!(
                    //     "successfully installed dependency package {} for root package {}",
                    //     &package_name, root_folder
                    // );
                } else {
                    // println!(
                    //     "package {} installation skipped already downloaded",
                    //     &package_name
                    // )
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

// fn get_package_dependencies(
//     path_lock: Option<&String>,
//     parent_dep: Option<&String>,
//     current_dep: &String,
//     current_dep_version: Option<&String>,
//     download_dir: &PathBuf,
//     package_lock: &PackageLock,
// ) {
//     let mut current_path_lock: Option<String> = None;
//     let directly_installed_package = package_lock
//         .packages
//         .get(&format!("node_modules/{}", current_dep))
//         .expect(&format!(
//             "package {current_dep} not found path searched: node_modules/{current_dep}"
//         ));
//     let current_package = if let Some(current_dep_version) = current_dep_version {
//         let version = Version::parse(&directly_installed_package.version).expect(&format!(
//             "something is wrong with the semantic versioning of package: {}",
//             current_dep
//         ));
//         let version_req = VersionReq::parse(current_dep_version).expect(&format!(
//             "something is wrong with the semantic versioning of package: {}",
//             current_dep
//         ));

//         if !version_req.matches(&version) {
//             let temp_current_path_lock = match path_lock {
//                 Some(path) => path.clone(),
//                 None => format!("node_modules/{}/node_modules/{}",parent_dep.expect(&format!("somehow a non root dependecy doesnt have a parent the non root dependency is {}",current_dep)),current_dep)
//             };

//             // print!("{:?},", &temp_current_path_lock);
//             // println!("`curr package {current_dep}, ver req {version_req} ver: {version}, path: {temp_current_path_lock}");
//             current_path_lock = Some(temp_current_path_lock.clone());
//             package_lock
//                 .packages
//                 .get(&temp_current_path_lock)
//                 .expect(&format!(
//                     "some package not found,\npackage {} path searched {}",
//                     current_dep, temp_current_path_lock
//                 ))
//         } else {
//             directly_installed_package
//         }
//     } else {
//         directly_installed_package
//     };

//     let all_deps_combined = merge_option_hashmaps!(
//         &current_package.dependencies,
//         &current_package.dev_dependencies
//     );

//     let mut parent_dependencies_names: Vec<&String> = Vec::new();
//     if current_path_lock.is_some() {
//         let parent_key = match path_lock {
//             Some(path) => {
//                 let parts: Vec<&str> = path.split('/').collect();
//                 if parts.len() > 2 {
//                     // Join all parts except the last two to get the prefix
//                     let prefix: String = parts[..parts.len() - 2].join("/");
//                     prefix
//                 }else {
//                     panic!("not supposed to happen")
//                 }
//             },
//             None => format!(
//                 "node_modules/{}",
//                 parent_dep.expect(&format!(
//                     "somehow a non root dependecy doesnt have a parent the non root dependency is {}",
//                     current_dep
//                 ))
//             ),
//         };
//         let parent_package = package_lock.packages.get(&parent_key).expect(&format!(
//             "some package not found path: {:?}, parent of package: {current_dep}",
//             &parent_key
//         ));

//         let parent_dependencies: [&Option<HashMap<String, String>>; 2] = [
//             &parent_package.dependencies,
//             &parent_package.dev_dependencies,
//         ];
//         parent_dependencies_names = parent_dependencies
//             .into_iter()
//             .flatten()
//             .flat_map(|map| map.keys())
//             .collect::<Vec<&String>>();
//     }

//     // println!("curr package {current_dep} deps = {:?}", &all_deps_combined);

//     if !all_deps_combined.is_empty() {
//         for (depndency, version) in all_deps_combined {
//             if depndency != current_dep {
//                 match &current_path_lock {
//                     Some(curr_path_lock_some) => {
//                         let is_dependency_in_parent = parent_dependencies_names
//                             .iter()
//                             .any(|&dep| dep == depndency);
//                         if is_dependency_in_parent {
//                             let result =
//                                 format!("{}/node_modules/{}", curr_path_lock_some, depndency);
//                             get_package_dependencies(
//                                 Some(&result),
//                                 Some(current_dep),
//                                 depndency,
//                                 Some(version),
//                                 download_dir,
//                                 package_lock,
//                             )
//                         } else {
//                             match curr_path_lock_some.rsplit_once('/') {
//                                 Some((prefix, _)) => {
//                                     // Construct the new string with the replacement
//                                     let result = format!("{}/{}", prefix, depndency);

//                                     get_package_dependencies(
//                                         Some(&result),
//                                         Some(current_dep),
//                                         depndency,
//                                         Some(version),
//                                         download_dir,
//                                         package_lock,
//                                     )
//                                 }
//                                 None => panic!("somehow we got here"),
//                             }
//                         }
//                     }
//                     None => get_package_dependencies(
//                         None,
//                         Some(current_dep),
//                         depndency,
//                         Some(version),
//                         download_dir,
//                         package_lock,
//                     ),
//                 }
//             }
//         }
//     }

//     if let Err(err) = fs::metadata(download_dir) {
//         fs::create_dir(download_dir).expect(&format!(
//             "failed creating path {:?} \n Error: {:?}",
//             download_dir, err
//         ))
//     }
//     let download_url = current_package.resolved.clone().expect(&format!(
//         "failed on packge {} failed to get the download url Disclaimer this should not happen",
//         current_dep
//     ));

//     println!("{:?},{download_url},{:?}", current_dep, download_dir);
//     let tarball_name = download_dir.join(format!(
//         "{package_name}-{version}.tgz",
//         version = &current_package.version,
//         package_name = &current_dep
//     ));

//     let is_package_installed = fs::metadata(&tarball_name);
//     if is_package_installed.is_err() {
//         let resp = reqwest::blocking::get(download_url).expect("request failed npm servers suck");
//         let body = resp.bytes().expect("body invalid");
//         let _ = std::fs::write(&tarball_name, &body);
//         let root_folder = download_dir
//             .file_name()
//             .and_then(|os_str| os_str.to_str())
//             .expect("should never happen");

//         println!(
//             "successfully installed dependency package {} for root package {}",
//             &current_dep, root_folder
//         );
//     } else {
//         println!(
//             "package {} installation skipped already downloaded",
//             &current_dep
//         )
//     }
// }
