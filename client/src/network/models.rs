use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    File,
    Directory,
}

#[derive(Deserialize)]
pub struct SerializableFSItem {
    pub name: String,
    pub item_type: ItemType,
}
