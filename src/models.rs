use serde::{Deserialize, Serialize};

// A mailing address.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Address {
    pub first_name: String,
    pub last_name: String,
    pub address1: String,
    pub address2: Option<String>,
    pub city: String,
    pub state: String,
    pub zip: String,
}
impl Address {
    pub fn new() -> Address {
        Address::default()
    }
}
