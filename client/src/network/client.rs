use super::models::*;
use reqwest::blocking::Client;
use urlencoding::encode;

pub fn list_path(path : &str) -> Option<Vec<SerializableFSItem>>{
    let client = Client::new();
    let encoded = encode(path);
    let url = format!("http://127.0.0.1:8080/list/{}", encoded);
    println!("DEBUG fetching {}", url);
    let resp = client
        .get(url)
        .send().ok()?
        .json::<Vec<SerializableFSItem>>().ok()?;
    Some(resp)
}