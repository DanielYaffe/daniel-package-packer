// Example code that deserializes and serializes the model.
// extern crate serde;
// #[macro_use]
// extern crate serde_derive;
// extern crate serde_json;
//
// use generated_module::Package_lock;
//
// fn main() {
//     let json = r#"{"answer": 42}"#;
//     let model: Package_lock = serde_json::from_str(&json).unwrap();
// }

use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageLock {
    pub name: String,
    pub version: String,
    pub lockfile_version: i64,
    pub requires: bool,
    pub packages: HashMap<String, Package>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub name: Option<String>,
    pub version: String,
    pub license: Option<String>,
    pub dependencies: Option<HashMap<String, String>>,
    pub dev_dependencies: Option<HashMap<String, String>>,
    pub resolved: Option<String>,
    pub integrity: Option<String>,
    pub dev: Option<bool>,
    // pub engines: Option<Engines>,
    pub peer_dependencies: Option<HashMap<String, String>>,
    // pub cpu: Option<Vec<String>>,
    // pub optional: Option<bool>,
    // pub os: Option<Vec<String>>,
    // pub has_install_script: Option<bool>,
    // pub dev_optional: Option<bool>,
    pub optional_dependencies: Option<HashMap<String, String>>,
    pub peer: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Engines {
    pub node: String,
    pub npm: Option<String>,
    pub iojs: Option<String>,
}
